import { gzipSync } from 'node:zlib'
import type { SystemMetricsExecutor } from './types.js'

const WINDOWS_METRICS_COMPLETE_MARKER = '__FILETERM_METRICS_COMPLETE__'

export async function runPowerShellMetricsScript(executor: SystemMetricsExecutor) {
  const script = buildWindowsMetricsScript()
  const commands = [buildPowerShellMetricsCommand('powershell', script), buildPowerShellMetricsCommand('pwsh', script)]
  let lastError: unknown

  for (const command of commands) {
    try {
      const raw = await executor.exec(command, { timeoutMs: 12000 })
      if (!raw.includes(WINDOWS_METRICS_COMPLETE_MARKER)) {
        throw new Error(`Windows metrics script did not emit ${WINDOWS_METRICS_COMPLETE_MARKER}`)
      }
      return raw
    } catch (error) {
      lastError = error
      if (isTimeoutError(error) || !isCommandUnavailable(error)) {
        break
      }
    }
  }

  throw lastError instanceof Error ? lastError : new Error(String(lastError))
}

export function buildPowerShellMetricsCommand(shell: 'powershell' | 'pwsh', script: string) {
  const compressedScript = gzipSync(Buffer.from(script, 'utf8')).toString('base64')
  const loader =
    `$b=[Convert]::FromBase64String('${compressedScript}');` +
    '$m=New-Object IO.MemoryStream(,$b);' +
    '$g=New-Object IO.Compression.GzipStream($m,[IO.Compression.CompressionMode]::Decompress);' +
    '$r=New-Object IO.StreamReader($g,[Text.Encoding]::UTF8);' +
    '& ([scriptblock]::Create($r.ReadToEnd()))'
  const command = `${shell} -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "${loader}"`
  if (command.length >= 8000) {
    throw new Error(`Windows metrics command exceeds the safe cmd.exe command-line budget (${command.length})`)
  }
  return command
}

function isCommandUnavailable(error: unknown) {
  return (
    error instanceof Error &&
    /(not recognized|not found|command not found|cannot find|is not installed|不是内部或外部命令|无法将.+识别)/i.test(
      error.message
    )
  )
}

function isTimeoutError(error: unknown) {
  return error instanceof Error && (error.name === 'TimeoutError' || /timed?\s*out|超时/i.test(error.message))
}

export function buildWindowsMetricsScript() {
  return `
& {
$ErrorActionPreference = "SilentlyContinue"
$ProgressPreference = "SilentlyContinue"
try {
  $utf8 = New-Object System.Text.UTF8Encoding($false)
  [Console]::OutputEncoding = $utf8
  $OutputEncoding = $utf8
} catch {}
$script:CimAvailable = [bool] (Get-Command Get-CimInstance -ErrorAction SilentlyContinue)
$script:WmiAvailable = [bool] (Get-Command Get-WmiObject -ErrorAction SilentlyContinue)
$script:WmicAvailable = [bool] (Get-Command wmic.exe -ErrorAction SilentlyContinue)
$script:CollectionDeadline = (Get-Date).AddSeconds(8)

function Write-Metric([string] $Name, [object] $Value) {
  if ($null -eq $Value) { $Value = "" }
  Write-Output ("__" + $Name + "__" + [string] $Value)
}

function Format-Bytes([double] $Bytes) {
  if ($Bytes -ge 1TB) { return ("{0:N1} TB" -f ($Bytes / 1TB)) }
  if ($Bytes -ge 1GB) { return ("{0:N1} GB" -f ($Bytes / 1GB)) }
  if ($Bytes -ge 1MB) { return ("{0:N1} MB" -f ($Bytes / 1MB)) }
  if ($Bytes -ge 1KB) { return ("{0:N0} KB" -f ($Bytes / 1KB)) }
  return ("{0:N0} B" -f $Bytes)
}

function Get-ManagementInstances([string] $ClassName, [string] $Filter = "") {
  if ((Get-Date) -ge $script:CollectionDeadline) { return @() }
  if ($script:CimAvailable) {
    try {
      $parameters = @{ ClassName = $ClassName; ErrorAction = "Stop"; OperationTimeoutSec = 2 }
      if ($Filter) { $parameters.Filter = $Filter }
      $rows = @(Get-CimInstance @parameters)
      if ($rows.Count -gt 0) { return $rows }
    } catch {
      $script:CimAvailable = $false
    }
  }
  if ($script:WmiAvailable) {
    $job = $null
    try {
      $parameters = @{ Class = $ClassName; ErrorAction = "Stop"; AsJob = $true }
      if ($Filter) {
        $parameters.Filter = $Filter
      }
      $job = Get-WmiObject @parameters
      if (-not (Wait-Job -Job $job -Timeout 2 -ErrorAction Stop)) {
        throw "WMI query timed out: $ClassName"
      }
      $rows = @(Receive-Job -Job $job -ErrorAction Stop)
      if ($rows.Count -gt 0) { return $rows }
    } catch {
      $script:WmiAvailable = $false
    } finally {
      if ($job) {
        Stop-Job -Job $job -ErrorAction SilentlyContinue
        Remove-Job -Job $job -Force -ErrorAction SilentlyContinue
      }
    }
  }
  return @()
}

function Invoke-WmicCsv([string[]] $Arguments) {
  if (-not $script:WmicAvailable -or (Get-Date) -ge $script:CollectionDeadline) { return @() }
  $process = $null
  try {
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = "wmic.exe"
    $startInfo.Arguments = (($Arguments + "/format:csv") -join " ")
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    if (-not $process.Start()) { throw "Unable to start wmic.exe" }
    if (-not $process.WaitForExit(2000)) {
      try { $process.Kill() } catch {}
      throw "WMIC query timed out"
    }
    $rows = @($process.StandardOutput.ReadToEnd() -split "\r?\n" | Where-Object { $_ -and $_.Trim() })
    if (-not $rows) { return @() }
    return @($rows | ConvertFrom-Csv)
  } catch {
    $script:WmicAvailable = $false
    return @()
  } finally {
    if ($process) { $process.Dispose() }
  }
}

function Convert-ManagementDate([object] $Value) {
  if ($Value -is [datetime]) { return [datetime] $Value }
  if (-not $Value) { return $null }
  try { return [System.Management.ManagementDateTimeConverter]::ToDateTime([string] $Value) } catch {}
  try { return [datetime] $Value } catch { return $null }
}

$os = @(Get-ManagementInstances "Win32_OperatingSystem") | Select-Object -First 1
if (-not $os) {
  $os = @(Invoke-WmicCsv @("os", "get", "Caption,Version,OSArchitecture,LastBootUpTime,TotalVisibleMemorySize,FreePhysicalMemory")) | Select-Object -First 1
}

$osCaption = if ($os -and $os.Caption) { [string] $os.Caption } else { [Environment]::OSVersion.VersionString }
$osVersion = if ($os -and $os.Version) { [string] $os.Version } else { [Environment]::OSVersion.Version.ToString() }
$osArchitecture = if ($os -and $os.OSArchitecture) { [string] $os.OSArchitecture } elseif ([Environment]::Is64BitOperatingSystem) { "64-bit" } else { "32-bit" }
$lastBootUpTime = if ($os) { Convert-ManagementDate $os.LastBootUpTime } else { $null }
if (-not $lastBootUpTime) {
  $lastBootUpTime = (Get-Date).AddMilliseconds(-[Environment]::TickCount64)
}

$processors = @(Get-ManagementInstances "Win32_Processor")
if (-not $processors -or $processors.Count -eq 0) {
  $processors = @(Invoke-WmicCsv @("cpu", "get", "Name,NumberOfCores,MaxClockSpeed,LoadPercentage"))
}
$reportedCpuLoads = @()
foreach ($processor in $processors) {
  $load = 0.0
  if ([double]::TryParse([string] $processor.LoadPercentage, [ref] $load)) {
    $reportedCpuLoads += $load
  }
}
$hasReportedCpuLoad = $reportedCpuLoads.Count -gt 0
if (-not $processors -or $processors.Count -eq 0) {
  $processors = @([pscustomobject]@{
    Name = if ($env:PROCESSOR_IDENTIFIER) { $env:PROCESSOR_IDENTIFIER } else { "Windows Processor" }
    NumberOfCores = [Environment]::ProcessorCount
    MaxClockSpeed = 0
    LoadPercentage = $null
  })
}
$cpuPercent = if ($hasReportedCpuLoad) { [int] (($reportedCpuLoads | Measure-Object -Average).Average) } else { 0 }
$uptimeSeconds = [math]::Max(0, [int] ((Get-Date) - $lastBootUpTime).TotalSeconds)

$memoryTotalBytes = if ($os) { [double] $os.TotalVisibleMemorySize * 1024 } else { 0 }
$memoryAvailableBytes = if ($os) { [double] $os.FreePhysicalMemory * 1024 } else { 0 }
if ($memoryTotalBytes -le 0) {
  try {
    Add-Type -AssemblyName Microsoft.VisualBasic -ErrorAction Stop
    $computerInfo = New-Object Microsoft.VisualBasic.Devices.ComputerInfo
    $memoryTotalBytes = [double] $computerInfo.TotalPhysicalMemory
    $memoryAvailableBytes = [double] $computerInfo.AvailablePhysicalMemory
  } catch {}
}
$memoryUsedBytes = [math]::Max([double] 0, $memoryTotalBytes - $memoryAvailableBytes)
$memoryPercent = if ($memoryTotalBytes -gt 0) { [int] (($memoryUsedBytes * 100) / $memoryTotalBytes) } else { 0 }

$pageFiles = @(Get-ManagementInstances "Win32_PageFileUsage")
if (-not $pageFiles -or $pageFiles.Count -eq 0) {
  $pageFiles = @(Invoke-WmicCsv @("pagefile", "get", "AllocatedBaseSize,CurrentUsage"))
}
$swapTotalBytes = [double] (($pageFiles | Measure-Object -Property AllocatedBaseSize -Sum).Sum) * 1MB
$swapUsedBytes = [double] (($pageFiles | Measure-Object -Property CurrentUsage -Sum).Sum) * 1MB
$swapAvailableBytes = [math]::Max([double] 0, $swapTotalBytes - $swapUsedBytes)
$swapPercent = if ($swapTotalBytes -gt 0) { [int] (($swapUsedBytes * 100) / $swapTotalBytes) } else { 0 }

$sshServerAddress = ""
try {
  $sshConnectionParts = @(([string] $env:SSH_CONNECTION).Trim() -split "\\s+")
  if (
    $sshConnectionParts.Count -ge 4 -and
    $sshConnectionParts[2] -match "^[0-9]+\\.[0-9]+\\.[0-9]+\\.[0-9]+$" -and
    $sshConnectionParts[2] -notlike "127.*" -and
    $sshConnectionParts[2] -notlike "169.254.*"
  ) {
    $sshServerAddress = [string] $sshConnectionParts[2]
  }
} catch {}

$ipCandidates = @()
try {
  [System.Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces() | Where-Object {
    $_.OperationalStatus -eq [System.Net.NetworkInformation.OperationalStatus]::Up -and
    $_.NetworkInterfaceType -ne [System.Net.NetworkInformation.NetworkInterfaceType]::Loopback
  } | ForEach-Object {
    $networkInterface = $_
    $properties = $networkInterface.GetIPProperties()
    $identity = (([string] $networkInterface.Name) + " " + ([string] $networkInterface.Description)).ToLowerInvariant()
    $isVirtual = $identity -match "tailscale|zerotier|vethernet|hyper-v|virtual|vmware|virtualbox|docker|wsl|bluetooth"
    $hasGateway = @($properties.GatewayAddresses | Where-Object {
      $gateway = $_.Address.IPAddressToString
      $gateway -match "^[0-9]+\\." -and $gateway -ne "0.0.0.0"
    }).Count -gt 0
    $isPhysicalType =
      $networkInterface.NetworkInterfaceType -eq [System.Net.NetworkInformation.NetworkInterfaceType]::Wireless80211 -or
      $networkInterface.NetworkInterfaceType -eq [System.Net.NetworkInformation.NetworkInterfaceType]::Ethernet -or
      $networkInterface.NetworkInterfaceType -eq [System.Net.NetworkInformation.NetworkInterfaceType]::GigabitEthernet -or
      $networkInterface.NetworkInterfaceType -eq [System.Net.NetworkInformation.NetworkInterfaceType]::FastEthernetFx -or
      $networkInterface.NetworkInterfaceType -eq [System.Net.NetworkInformation.NetworkInterfaceType]::FastEthernetT

    $properties.UnicastAddresses | ForEach-Object {
      $address = $_.Address.IPAddressToString
      if ($address -notmatch "^[0-9]+\\." -or $address -like "127.*" -or $address -like "169.254.*") { return }
      $score = 0
      if (-not $isVirtual) { $score += 100 } else { $score -= 200 }
      if ($hasGateway) { $score += 80 }
      if ($isPhysicalType) { $score += 30 }
      if ($address -match "^(10\\.|192\\.168\\.|172\\.(1[6-9]|2[0-9]|3[01])\\.)") { $score += 40 }
      if ($address -eq $sshServerAddress) { $score += 25 }
      $ipCandidates += [pscustomobject]@{ Address = $address; Score = $score }
    }
  }
} catch {}

$ip = [string] (($ipCandidates | Sort-Object -Property Score -Descending | Select-Object -First 1).Address)
$addresses = @()
if (-not $ip) {
  try {
    $addresses = @([System.Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces() | Where-Object {
      $_.OperationalStatus -eq [System.Net.NetworkInformation.OperationalStatus]::Up -and
      $_.NetworkInterfaceType -ne [System.Net.NetworkInformation.NetworkInterfaceType]::Loopback
    } | ForEach-Object {
      $_.GetIPProperties().UnicastAddresses | ForEach-Object { $_.Address.IPAddressToString }
    } | Where-Object { $_ -match "^[0-9]+\\." -and $_ -notlike "127.*" -and $_ -notlike "169.254.*" })
  } catch {}
}
if (-not $ip -and (-not $addresses -or $addresses.Count -eq 0)) {
  try {
    $addresses = @(Get-ManagementInstances "Win32_NetworkAdapterConfiguration" | Where-Object { $_.IPEnabled } | ForEach-Object { $_.IPAddress } | Where-Object { $_ -and $_ -match "^[0-9]+\\." -and $_ -notlike "127.*" })
  } catch {}
}
if (-not $ip -and (-not $addresses -or $addresses.Count -eq 0)) {
  try {
    $ipconfigOutput = (& ipconfig.exe 2>$null) -join [Environment]::NewLine
    $ipMatch = [regex]::Match($ipconfigOutput, "IPv4[^:]*:\\s*([0-9.]+)")
    if ($ipMatch.Success) { $addresses = @($ipMatch.Groups[1].Value) }
  } catch {}
}
if (-not $ip) {
  $ip = $addresses | Select-Object -First 1
}

$netBefore = @{}
try {
  [System.Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces() | Where-Object {
    $_.OperationalStatus -eq [System.Net.NetworkInformation.OperationalStatus]::Up -and
    $_.NetworkInterfaceType -ne [System.Net.NetworkInformation.NetworkInterfaceType]::Loopback
  } | ForEach-Object {
    $stats = $_.GetIPv4Statistics()
    $netBefore[$_.Name] = [pscustomobject]@{ Rx = [double] $stats.BytesReceived; Tx = [double] $stats.BytesSent }
  }
} catch {}
$sampleStartedAt = Get-Date
$processCpuBefore = $null
$processCpuBeforeById = @{}
try {
  $processSnapshotBefore = @(Get-Process -ErrorAction Stop)
  $processCpuBefore = [double] (($processSnapshotBefore | Measure-Object -Property CPU -Sum).Sum)
  foreach ($process in $processSnapshotBefore) {
    $cpuSeconds = 0.0
    if ([double]::TryParse([string] $process.CPU, [ref] $cpuSeconds)) {
      $processCpuBeforeById[[int] $process.Id] = $cpuSeconds
    }
  }
} catch {}
Start-Sleep -Milliseconds 250
$netAfter = @()
try {
  $netAfter = @([System.Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces() | Where-Object {
    $_.OperationalStatus -eq [System.Net.NetworkInformation.OperationalStatus]::Up -and
    $_.NetworkInterfaceType -ne [System.Net.NetworkInformation.NetworkInterfaceType]::Loopback
  } | ForEach-Object {
    $stats = $_.GetIPv4Statistics()
    [pscustomobject]@{ Name = $_.Name; ReceivedBytes = [double] $stats.BytesReceived; SentBytes = [double] $stats.BytesSent }
  })
} catch {}
$sampleMs = [math]::Max(1, [int] ((Get-Date) - $sampleStartedAt).TotalMilliseconds)
$processorCount = [math]::Max(1, [Environment]::ProcessorCount)
$processSnapshotAfter = @()
$processCpuPercentById = @{}
$processCpuAfter = $null
try {
  $processSnapshotAfter = @(Get-Process -ErrorAction Stop)
  $processCpuAfter = [double] (($processSnapshotAfter | Measure-Object -Property CPU -Sum).Sum)
  foreach ($process in $processSnapshotAfter) {
    $processId = [int] $process.Id
    $cpuSecondsAfter = 0.0
    if (
      $processCpuBeforeById.ContainsKey($processId) -and
      [double]::TryParse([string] $process.CPU, [ref] $cpuSecondsAfter)
    ) {
      $cpuDelta = [math]::Max([double] 0, $cpuSecondsAfter - [double] $processCpuBeforeById[$processId])
      $processCpuPercentById[$processId] = [math]::Min(
        [double] 100,
        ($cpuDelta * 100000) / ($sampleMs * $processorCount)
      )
    }
  }
} catch {}
if (-not $hasReportedCpuLoad -and $null -ne $processCpuBefore -and $null -ne $processCpuAfter) {
  try {
    $cpuPercent = [int] ([math]::Round(
      ([math]::Max([double] 0, $processCpuAfter - $processCpuBefore) * 100000) / ($sampleMs * $processorCount)
    ))
  } catch {}
}
if ($cpuPercent -lt 0) { $cpuPercent = 0 }
if ($cpuPercent -gt 100) { $cpuPercent = 100 }
$idlePercent = [math]::Max(0, 100 - $cpuPercent)
$logicalProcessorCount = [math]::Max(1, [Environment]::ProcessorCount)
$systemLoad = [string]::Format(
  [Globalization.CultureInfo]::InvariantCulture,
  "{0:0.00}",
  ($cpuPercent * $logicalProcessorCount) / 100
)
$rxRate = 0
$txRate = 0
$ifaces = @()
$ifaceRows = @()
foreach ($item in $netAfter) {
  $name = [string] $item.Name
  if ([string]::IsNullOrWhiteSpace($name)) { continue }
  $before = $netBefore[$name]
  $rxTotal = [double] $item.ReceivedBytes
  $txTotal = [double] $item.SentBytes
  $rowRxRate = 0
  $rowTxRate = 0
  if ($before) {
    $rowRxRate = [math]::Max(0, [int] (($rxTotal - $before.Rx) * 1000 / $sampleMs))
    $rowTxRate = [math]::Max(0, [int] (($txTotal - $before.Tx) * 1000 / $sampleMs))
  }
  $rxRate += $rowRxRate
  $txRate += $rowTxRate
  $ifaces += $name
  $ifaceRows += ("{0}|{1}|{2}|{3}|{4}" -f $name, [int64] $rxTotal, [int64] $txTotal, $rowRxRate, $rowTxRate)
}

$disks = @(Get-ManagementInstances "Win32_LogicalDisk" "DriveType=3")
if (-not $disks -or $disks.Count -eq 0) {
  $disks = @(Invoke-WmicCsv @("logicaldisk", "where", "DriveType=3", "get", "DeviceID,FreeSpace,Size"))
}
if (-not $disks -or $disks.Count -eq 0) {
  try {
    $disks = @([System.IO.DriveInfo]::GetDrives() | Where-Object { $_.IsReady -and $_.DriveType -eq [System.IO.DriveType]::Fixed } | ForEach-Object {
      [pscustomobject]@{ DeviceID = $_.Name.TrimEnd("\\"); Size = [double] $_.TotalSize; FreeSpace = [double] $_.AvailableFreeSpace }
    })
  } catch {}
}
$diskRows = @()
$fileSystemRows = @()
foreach ($disk in $disks) {
  $name = [string] $disk.DeviceID
  $size = [double] $disk.Size
  $free = [double] $disk.FreeSpace
  $used = [math]::Max([double] 0, $size - $free)
  $percent = if ($size -gt 0) { [int] (($used * 100) / $size) } else { 0 }
  $diskRows += ("{0}|{1}/{2}" -f $name, (Format-Bytes $free), (Format-Bytes $size))
  $fileSystemRows += ("{0}|{1}|{2}|{3}%|{4}|{5}" -f $name, (Format-Bytes $size), (Format-Bytes $used), $percent, (Format-Bytes $free), $name)
}

$cpuRows = @()
foreach ($processor in $processors) {
  $cpuRows += ("{0}|{1}|{2}|-|-" -f ([string] $processor.Name).Trim(), [int] $processor.NumberOfCores, [string] $processor.MaxClockSpeed)
}

$gpuRows = @()
try {
  if ((Get-Date) -ge $script:CollectionDeadline) { throw "Collection budget exhausted" }
  Get-ManagementInstances "Win32_VideoController" | ForEach-Object {
    $memory = if ($_.AdapterRAM) { Format-Bytes ([double] $_.AdapterRAM) } else { "-" }
    $gpuRows += ("{0}|{1}|{2}|{3}" -f ([string] $_.Name).Trim(), ([string] $_.AdapterCompatibility).Trim(), ([string] $_.DriverVersion).Trim(), $memory)
  }
} catch {}

$processRows = @()
try {
  if ((Get-Date) -ge $script:CollectionDeadline) { throw "Collection budget exhausted" }
  $processSource = $processSnapshotAfter
  if (-not $processSource -or $processSource.Count -eq 0) {
    $processSource = @(Get-Process -ErrorAction Stop)
  }
  $processSource | Sort-Object -Property WorkingSet64 -Descending | Select-Object -First 80 | ForEach-Object {
    $elapsed = 0
    if ($_.StartTime) {
      $elapsed = [int] ((Get-Date) - $_.StartTime).TotalSeconds
    }
    $cpu = if ($processCpuPercentById.ContainsKey([int] $_.Id)) { [double] $processCpuPercentById[[int] $_.Id] } else { 0 }
    $processRows += ("{0:N1}M|{1:N1}|{2}|{3}" -f ($_.WorkingSet64 / 1MB), $cpu, $elapsed, $_.ProcessName)
  }
} catch {}
if ($processRows.Count -eq 0) {
  try {
    & tasklist.exe /FO CSV /NH 2>$null | ConvertFrom-Csv -Header ImageName,PID,SessionName,SessionNumber,MemUsage | Select-Object -First 80 | ForEach-Object {
      $memoryKb = [double] (([string] $_.MemUsage) -replace "[^0-9]", "")
      $command = [System.IO.Path]::GetFileNameWithoutExtension([string] $_.ImageName)
      $processRows += ("{0:N1}M|0.0|0|{1}" -f ($memoryKb / 1024), $command)
    }
  } catch {}
}

Write-Metric "PLATFORM" "windows"
Write-Metric "OS" $osCaption
Write-Metric "KERNEL_NAME" "Windows"
Write-Metric "KERNEL_VERSION" $osVersion
Write-Metric "ARCH" $osArchitecture
Write-Metric "HOSTNAME" $env:COMPUTERNAME
Write-Metric "IP" $ip
Write-Metric "UPTIME" ""
Write-Metric "UPTIME_SECONDS" $uptimeSeconds
Write-Metric "LOAD" $systemLoad
Write-Metric "LOAD_UNIT" "busy-logical-processors"
Write-Metric "CPU" $cpuPercent
Write-Metric "CPU_USAGE" ("0|{0}|0|{1}|0|0|0|0" -f $cpuPercent, $idlePercent)
Write-Metric "MEM" ("{0}|{1}|{2}|0|0|0" -f [int]($memoryUsedBytes / 1MB), [int]($memoryTotalBytes / 1MB), $memoryPercent)
Write-Metric "MEM_BYTES" ("{0}|{1}|{2}|{3}|0|0|0" -f [int64]$memoryUsedBytes, [int64]$memoryTotalBytes, [int64]$memoryAvailableBytes, $memoryPercent)
Write-Metric "SWAP" ("{0}|{1}|{2}" -f [int]($swapUsedBytes / 1MB), [int]($swapTotalBytes / 1MB), $swapPercent)
Write-Metric "SWAP_BYTES" ("{0}|{1}|{2}|{3}" -f [int64]$swapUsedBytes, [int64]$swapTotalBytes, [int64]$swapAvailableBytes, $swapPercent)
Write-Output "__CPUINFO_START__"
$cpuRows | ForEach-Object { Write-Output $_ }
Write-Output "__CPUINFO_END__"
Write-Output "__GPUINFO_START__"
$gpuRows | ForEach-Object { Write-Output $_ }
Write-Output "__GPUINFO_END__"
Write-Metric "IFACES" ($ifaces -join ",")
Write-Metric "ACTIVE_IFACE" ($ifaces | Select-Object -First 1)
Write-Metric "RATES" ("{0}|{1}" -f $rxRate, $txRate)
Write-Output "__IFACE_RATES_START__"
$ifaceRows | ForEach-Object { Write-Output $_ }
Write-Output "__IFACE_RATES_END__"
Write-Output "__DISK_START__"
$diskRows | ForEach-Object { Write-Output $_ }
Write-Output "__DISK_END__"
Write-Output "__FILESYSTEMS_START__"
$fileSystemRows | ForEach-Object { Write-Output $_ }
Write-Output "__FILESYSTEMS_END__"
Write-Output "__PROCS_START__"
$processRows | ForEach-Object { Write-Output $_ }
Write-Output "__PROCS_END__"
Write-Output "${WINDOWS_METRICS_COMPLETE_MARKER}"
}
`
}
