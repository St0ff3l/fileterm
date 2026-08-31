/// PowerShell-based metrics script for Windows remotes.
/// Emits the same `__KEY__VALUE` markers as `build_posix_metrics_command`.
pub fn build_windows_metrics_command() -> String {
    r#"
$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}

function Write-Metric([string]$Name, [object]$Value) {
    if ($null -eq $Value) { $Value = '' }
    Write-Output ('__' + $Name + '__' + [string]$Value)
}

function Format-FileTermBytes([double]$Bytes) {
    $culture = [Globalization.CultureInfo]::InvariantCulture
    if ($Bytes -ge 1TB) { return [string]::Format($culture, '{0:0.0} TB', $Bytes / 1TB) }
    if ($Bytes -ge 1GB) { return [string]::Format($culture, '{0:0.0} GB', $Bytes / 1GB) }
    if ($Bytes -ge 1MB) { return [string]::Format($culture, '{0:0.0} MB', $Bytes / 1MB) }
    if ($Bytes -ge 1KB) { return [string]::Format($culture, '{0:0.0} KB', $Bytes / 1KB) }
    return [string]::Format($culture, '{0:0} B', $Bytes)
}

function Get-CpuUsagePercent {
    # Get-Counter rejects sub-second SampleInterval values on Windows. Use the
    # .NET performance counter directly so the initial snapshot is useful and
    # does not silently collapse to 0% on localized Windows installations.
    try {
        $counter = New-Object Diagnostics.PerformanceCounter('Processor', '% Processor Time', '_Total')
        $null = $counter.NextValue()
        Start-Sleep -Milliseconds 500
        $value = [double]$counter.NextValue()
        if ($value -ge 0 -and $value -le 100) {
            return [Math]::Round($value)
        }
    } catch {}

    # Keep a CIM fallback for Server Core/minimal images where the performance
    # counter category is unavailable.
    try {
        $loads = @(Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue |
            Where-Object { $null -ne $_.LoadPercentage } |
            ForEach-Object { [double]$_.LoadPercentage })
        if ($loads.Count -gt 0) {
            return [Math]::Round((($loads | Measure-Object -Average).Average))
        }
    } catch {}

    return 0
}

$os = Get-CimInstance Win32_OperatingSystem
$cs = Get-CimInstance Win32_ComputerSystem
$cpu = Get-CimInstance Win32_Processor
$memTotal = [double]$os.TotalVisibleMemorySize * 1KB
$memFree  = [double]$os.FreePhysicalMemory  * 1KB
$memUsed  = $memTotal - $memFree
$memPct   = if ($memTotal -gt 0) { [Math]::Round($memUsed * 100 / $memTotal) } else { 0 }

$swapTotal = [double]$os.TotalVirtualMemorySize * 1KB
$swapFree  = [double]$os.FreeVirtualMemory      * 1KB
$swapUsed  = $swapTotal - $swapFree
$swapPct   = if ($swapTotal -gt 0) { [Math]::Round($swapUsed * 100 / $swapTotal) } else { 0 }

# CPU usage sampled over 0.5s
$cpuPct = Get-CpuUsagePercent
$logicalProcessorCount = [Math]::Max(1, [Environment]::ProcessorCount)
$systemLoad = [string]::Format(
    [Globalization.CultureInfo]::InvariantCulture,
    '{0:0.00}',
    ($cpuPct * $logicalProcessorCount) / 100
)

$hostname = $env:COMPUTERNAME
$ip = ''
$sshConnectionParts = @(([string]$env:SSH_CONNECTION).Trim() -split '\s+')
if ($sshConnectionParts.Count -ge 4) { $ip = [string]$sshConnectionParts[2] }
if (-not $ip) {
    $net = Get-NetIPConfiguration -ErrorAction SilentlyContinue | Where-Object { $_.IPv4DefaultGateway -ne $null } | Select-Object -First 1
    if ($net) { $ip = $net.IPv4Address.IPAddress }
}

$uptimeSec = 0
if ($os.LastBootUpTime) {
    $uptimeSec = [int]((Get-Date) - $os.LastBootUpTime).TotalSeconds
}

$cpuCores = ($cpu | Measure-Object NumberOfLogicalProcessors -Sum).Sum
if (-not $cpuCores) { $cpuCores = 0 }
$cpuRows = @()
foreach ($processor in @($cpu)) {
    $cpuModel = ([string]$processor.Name).Trim()
    $cpuFrequency = if ([double]$processor.MaxClockSpeed -gt 0) { [string][int]$processor.MaxClockSpeed } else { '-' }
    $cacheParts = @()
    if ([double]$processor.L2CacheSize -gt 0) { $cacheParts += ('L2 ' + (Format-FileTermBytes ([double]$processor.L2CacheSize * 1KB))) }
    if ([double]$processor.L3CacheSize -gt 0) { $cacheParts += ('L3 ' + (Format-FileTermBytes ([double]$processor.L3CacheSize * 1KB))) }
    $cpuCache = if ($cacheParts.Count -gt 0) { $cacheParts -join ' / ' } else { '-' }
    $cpuRows += ('{0}|{1}|{2}|{3}|-' -f $cpuModel, $cpuCores, $cpuFrequency, $cpuCache)
}
if ($cpuRows.Count -eq 0) { $cpuRows += ('-|{0}|-|-|-' -f $cpuCores) }

function Convert-GpuMetricText([object]$Value, [string]$Unit) {
    $text = ([string]$Value).Trim()
    if (-not $text -or $text -eq '-' -or $text -match '^(?:\[?N/?A\]?|NA)$') { return '-' }
    return $text + ' ' + $Unit
}

function Get-GpuRuntimeMap {
    $runtime = @{}
    try {
        $runtimeLines = @(nvidia-smi --query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,power.limit --format=csv,noheader,nounits 2>$null)
        foreach ($line in $runtimeLines) {
            $parts = @(([string]$line) -split ',')
            if ($parts.Count -lt 7) { continue }
            $name = ([string]$parts[0]).Trim()
            if (-not $name) { continue }
            $runtime[$name.ToLowerInvariant()] = @{
                usage = Convert-GpuMetricText $parts[1] '%'
                memoryUsed = Convert-GpuMetricText $parts[2] 'MiB'
                memoryTotal = Convert-GpuMetricText $parts[3] 'MiB'
                temperature = Convert-GpuMetricText $parts[4] 'C'
                powerUsage = Convert-GpuMetricText $parts[5] 'W'
                powerLimit = Convert-GpuMetricText $parts[6] 'W'
            }
        }
    } catch {}
    return $runtime
}

function Get-GpuRows([object[]]$Adapters) {
    $runtime = Get-GpuRuntimeMap
    $rows = @()
    foreach ($adapter in @($Adapters)) {
        $gpuName = ([string]$adapter.Name).Trim()
        if (-not $gpuName) { continue }
        $gpuVendor = ([string]$adapter.AdapterCompatibility).Trim()
        if (-not $gpuVendor) { $gpuVendor = '-' }
        $gpuDriver = ([string]$adapter.DriverVersion).Trim()
        if (-not $gpuDriver) { $gpuDriver = '-' }
        $gpuMemory = if ([double]$adapter.AdapterRAM -gt 0) { Format-FileTermBytes ([double]$adapter.AdapterRAM) } else { '-' }
        $runtimeEntry = $null
        $gpuNameKey = $gpuName.ToLowerInvariant()
        if ($runtime.ContainsKey($gpuNameKey)) {
            $runtimeEntry = $runtime[$gpuNameKey]
        } else {
            foreach ($runtimeName in @($runtime.Keys)) {
                if ($gpuNameKey.Contains([string]$runtimeName) -or ([string]$runtimeName).Contains($gpuNameKey)) {
                    $runtimeEntry = $runtime[$runtimeName]
                    break
                }
            }
        }
        if ($runtimeEntry) {
            # Win32_VideoController.AdapterRAM is truncated to 4 GB on some
            # WDDM laptop drivers. nvidia-smi reports the physical VRAM, so
            # prefer its runtime total whenever it is available.
            if ($runtimeEntry.memoryTotal -ne '-') { $gpuMemory = $runtimeEntry.memoryTotal }
            $rows += ('{0}|{1}|{2}|{3}|{4}|{5}|{6}|{7}|{8}' -f $gpuName, $gpuVendor, $gpuDriver, $gpuMemory, $runtimeEntry.usage, $runtimeEntry.memoryUsed, $runtimeEntry.temperature, $runtimeEntry.powerUsage, $runtimeEntry.powerLimit)
        } else {
            $rows += ('{0}|{1}|{2}|{3}|-|-|-|-|-' -f $gpuName, $gpuVendor, $gpuDriver, $gpuMemory)
        }
    }
    return $rows
}

$gpuAdapters = @(Get-CimInstance Win32_VideoController)
$gpuRows = @(Get-GpuRows -Adapters $gpuAdapters)

$disks = Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3'
$diskLines = @()
$fsLines = @()
foreach ($d in $disks) {
    $size = [double]$d.Size
    $free = [double]$d.FreeSpace
    $used = $size - $free
    $pct  = if ($size -gt 0) { [Math]::Round($used * 100 / $size) } else { 0 }
    $sizeStr = Format-FileTermBytes $size
    $usedStr = Format-FileTermBytes $used
    $freeStr = Format-FileTermBytes $free
    $diskLines += ('{0}|{1}/{2}' -f $d.DeviceID, $usedStr, $sizeStr)
    $fsLines   += ('{0}|{1}|{2}|{3}%|{4}|{5}' -f $d.DeviceID, $sizeStr, $usedStr, $pct, $freeStr, $d.DeviceID)
}

$procs = Get-Process | Sort-Object -Property WS -Descending | Select-Object -First 20
$procLines = @()
foreach ($p in $procs) {
    $memMB = [Math]::Round($p.WorkingSet64 / 1MB, 1)
    $procLines += ('{0}||{1}M|0|0|{2}' -f $p.Id, $memMB, $p.ProcessName)
}

$ifaces = (Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'Up' } | Select-Object -ExpandProperty Name) -join ','
$rx1 = 0; $tx1 = 0
$ifStats = @{}
foreach ($i in (Get-NetAdapterStatistics -ErrorAction SilentlyContinue)) {
    $ifStats[$i.Name] = @{ rx = $i.ReceivedBytes; tx = $i.SentBytes }
    $rx1 += $i.ReceivedBytes
    $tx1 += $i.SentBytes
}
Start-Sleep -Milliseconds 500
$rx2 = 0; $tx2 = 0
$ifRates = @()
foreach ($i in (Get-NetAdapterStatistics -ErrorAction SilentlyContinue)) {
    $rx2 += $i.ReceivedBytes
    $tx2 += $i.SentBytes
    $prev = $ifStats[$i.Name]
    if ($prev) {
        $rxRate = ($i.ReceivedBytes - $prev.rx) * 2
        $txRate = ($i.SentBytes   - $prev.tx) * 2
        $ifRates += ('{0}|{1}|{2}|{3}|{4}' -f $i.Name, $i.ReceivedBytes, $i.SentBytes, $rxRate, $txRate)
    }
}
$rxRate = ($rx2 - $rx1) * 2
$txRate = ($tx2 - $tx1) * 2

Write-Output ('__PLATFORM__windows')
Write-Output ('__OS__' + $os.Caption)
Write-Output ('__KERNEL_NAME__Windows')
Write-Output ('__KERNEL_VERSION__' + $os.Version)
Write-Output ('__ARCH__' + $env:PROCESSOR_ARCHITECTURE)
Write-Output ('__HOSTNAME__' + $hostname)
Write-Output ('__IP__' + $ip)
Write-Output '__UPTIME__'
Write-Output ('__UPTIME_SECONDS__' + $uptimeSec)
Write-Output ('__LOAD__' + $systemLoad)
Write-Output '__LOAD_UNIT__busy-logical-processors'
Write-Output ('__CPU__' + $cpuPct)
Write-Output ('__CPU_USAGE__{0}|{1}|0|{2}|0|0|0|0' -f $cpuPct, $cpuPct, [Math]::Max(0, 100 - $cpuPct))
Write-Output ('__MEM__{0}|{1}|{2}|0|0|0' -f [Math]::Round($memUsed / 1MB), [Math]::Round($memTotal / 1MB), $memPct)
Write-Output ('__MEM_BYTES__{0}|{1}|{2}|{3}|0|0|0' -f $memUsed, $memTotal, $memFree, $memPct)
Write-Output ('__SWAP__{0}|{1}|{2}' -f [Math]::Round($swapUsed / 1MB), [Math]::Round($swapTotal / 1MB), $swapPct)
Write-Output ('__SWAP_BYTES__{0}|{1}|{2}|{3}' -f $swapUsed, $swapTotal, $swapFree, $swapPct)
Write-Output '__CPUINFO_START__'
$cpuRows | ForEach-Object { Write-Output $_ }
Write-Output '__CPUINFO_END__'
Write-Output '__GPUINFO_START__'
$gpuRows | ForEach-Object { Write-Output $_ }
Write-Output '__GPUINFO_END__'
Write-Output ('__IFACES__' + $ifaces)
Write-Output '__ACTIVE_IFACE__all'
Write-Output ('__RATES__{0}|{1}' -f $rxRate, $txRate)
Write-Output '__IFACE_RATES_START__'
$ifRates | ForEach-Object { Write-Output $_ }
Write-Output '__IFACE_RATES_END__'
Write-Output '__DISK_START__'
$diskLines | ForEach-Object { Write-Output $_ }
Write-Output '__DISK_END__'
Write-Output '__FILESYSTEMS_START__'
$fsLines | ForEach-Object { Write-Output $_ }
Write-Output '__FILESYSTEMS_END__'
Write-Output '__PROCS_START__'
$procLines | ForEach-Object { Write-Output $_ }
Write-Output '__PROCS_END__'
Write-Output '__FILETERM_METRICS_COMPLETE__'
"#.to_string()
}

/// Builds a long-lived Windows collector. The first block is the full system
/// snapshot; later blocks reuse cached static data and warm performance
/// counters so CPU/memory/network samples are emitted on a fixed clock without
/// paying PowerShell/CIM startup cost on every refresh.
pub fn build_windows_streaming_metrics_command(interval_seconds: u64) -> String {
    let mut script = build_windows_metrics_command();
    script.push_str(
        r#"
$cpuCounter = $null
$memoryAvailableCounter = $null
try {
    $cpuCounter = New-Object Diagnostics.PerformanceCounter('Processor', '% Processor Time', '_Total')
    $memoryAvailableCounter = New-Object Diagnostics.PerformanceCounter('Memory', 'Available Bytes')
    $null = $cpuCounter.NextValue()
    $null = $memoryAvailableCounter.NextValue()
} catch {}

$previousNetworkStats = @{}
foreach ($item in @(Get-NetAdapterStatistics -ErrorAction SilentlyContinue)) {
    $previousNetworkStats[[string]$item.Name] = @{
        rx = [double]$item.ReceivedBytes
        tx = [double]$item.SentBytes
    }
}
$previousProcCpuTimes = @{}
$sampleClock = [Diagnostics.Stopwatch]::StartNew()
$previousNetworkSampleMs = [double]$sampleClock.ElapsedMilliseconds
$previousProcSampleMs = [double]$sampleClock.ElapsedMilliseconds
$nextEmitMs = [double]$sampleClock.ElapsedMilliseconds + 1000
Write-Output '__FILETERM_METRICS_BLOCK__'

while ($true) {
    if ($cpuCounter) {
        try { $cpuPct = [Math]::Round($cpuCounter.NextValue()) } catch {}
    }
    if ($memoryAvailableCounter) {
        try { $memFree = [double]$memoryAvailableCounter.NextValue() } catch {}
    }
    $cpuPct = [Math]::Max(0, [Math]::Min(100, [double]$cpuPct))
    $memUsed = [Math]::Max(0, $memTotal - $memFree)
    $memPct = if ($memTotal -gt 0) { [Math]::Round($memUsed * 100 / $memTotal) } else { 0 }
    $systemLoad = [string]::Format(
        [Globalization.CultureInfo]::InvariantCulture,
        '{0:0.00}',
        ($cpuPct * $logicalProcessorCount) / 100
    )
    if ($os.LastBootUpTime) {
        $uptimeSec = [int]((Get-Date) - $os.LastBootUpTime).TotalSeconds
    }

    $networkNowMs = [double]$sampleClock.ElapsedMilliseconds
    $networkElapsedSeconds = [Math]::Max(0.001, ($networkNowMs - $previousNetworkSampleMs) / 1000)
    $previousNetworkSampleMs = $networkNowMs
    $rxRate = 0
    $txRate = 0
    $ifRates = @()
    $currentNetworkStats = @(Get-NetAdapterStatistics -ErrorAction SilentlyContinue)
    foreach ($item in $currentNetworkStats) {
        $name = [string]$item.Name
        $rxTotal = [double]$item.ReceivedBytes
        $txTotal = [double]$item.SentBytes
        $previous = $previousNetworkStats[$name]
        $itemRxRate = 0
        $itemTxRate = 0
        if ($previous) {
            $itemRxRate = [Math]::Max(0, ($rxTotal - [double]$previous.rx) / $networkElapsedSeconds)
            $itemTxRate = [Math]::Max(0, ($txTotal - [double]$previous.tx) / $networkElapsedSeconds)
        }
        $previousNetworkStats[$name] = @{ rx = $rxTotal; tx = $txTotal }
        $rxRate += $itemRxRate
        $txRate += $itemTxRate
        $ifRates += ('{0}|{1}|{2}|{3}|{4}' -f $name, $rxTotal, $txTotal, [Math]::Round($itemRxRate), [Math]::Round($itemTxRate))
    }

    $procSampleMs = [double]$sampleClock.ElapsedMilliseconds
    $procElapsedSeconds = [Math]::Max(0.001, ($procSampleMs - $previousProcSampleMs) / 1000)
    $previousProcSampleMs = $procSampleMs

    $procLines = @()
    $currentProcCpuTimes = @{}
    Get-Process -ErrorAction SilentlyContinue |
        Sort-Object -Property WorkingSet64 -Descending |
        Select-Object -First 20 |
        ForEach-Object {
            $memMB = [Math]::Round($_.WorkingSet64 / 1MB, 1)
            $currentCpu = if ($_.CPU) { [double]$_.CPU } else { 0 }
            $procId = [string]$_.Id
            $currentProcCpuTimes[$procId] = $currentCpu
            $prevCpu = $previousProcCpuTimes[$procId]
            $processCpuPct = if ($null -ne $prevCpu) { [Math]::Max(0, [Math]::Round((([double]$currentCpu - [double]$prevCpu) / $procElapsedSeconds) * 100 / $logicalProcessorCount, 1)) } else { 0 }
            $procLines += ('{0}||{1}M|{2}|0|{3}' -f $_.Id, $memMB, $processCpuPct, $_.ProcessName)
        }
    $previousProcCpuTimes = $currentProcCpuTimes
    $gpuRows = @(Get-GpuRows -Adapters $gpuAdapters)

    $waitMs = [Math]::Round($nextEmitMs - [double]$sampleClock.ElapsedMilliseconds)
    if ($waitMs -gt 0) { Start-Sleep -Milliseconds $waitMs }
    $nextEmitMs += 1000
    if ($nextEmitMs -le [double]$sampleClock.ElapsedMilliseconds) {
        $nextEmitMs = [double]$sampleClock.ElapsedMilliseconds + 1000
    }

    Write-Output '__PLATFORM__windows'
    Write-Output ('__OS__' + $os.Caption)
    Write-Output '__KERNEL_NAME__Windows'
    Write-Output ('__KERNEL_VERSION__' + $os.Version)
    Write-Output ('__ARCH__' + $env:PROCESSOR_ARCHITECTURE)
    Write-Output ('__HOSTNAME__' + $hostname)
    Write-Output ('__IP__' + $ip)
    Write-Output '__UPTIME__'
    Write-Output ('__UPTIME_SECONDS__' + $uptimeSec)
    Write-Output ('__LOAD__' + $systemLoad)
    Write-Output '__LOAD_UNIT__busy-logical-processors'
    Write-Output ('__CPU__' + $cpuPct)
    Write-Output ('__CPU_USAGE__0|{0}|0|{1}|0|0|0|0' -f $cpuPct, [Math]::Max(0, 100 - $cpuPct))
    Write-Output ('__MEM__{0}|{1}|{2}|0|0|0' -f [Math]::Round($memUsed / 1MB), [Math]::Round($memTotal / 1MB), $memPct)
    Write-Output ('__MEM_BYTES__{0}|{1}|{2}|{3}|0|0|0' -f $memUsed, $memTotal, $memFree, $memPct)
    Write-Output ('__SWAP__{0}|{1}|{2}' -f [Math]::Round($swapUsed / 1MB), [Math]::Round($swapTotal / 1MB), $swapPct)
    Write-Output ('__SWAP_BYTES__{0}|{1}|{2}|{3}' -f $swapUsed, $swapTotal, $swapFree, $swapPct)
    Write-Output '__CPUINFO_START__'
    $cpuRows | ForEach-Object { Write-Output $_ }
    Write-Output '__CPUINFO_END__'
    Write-Output '__GPUINFO_START__'
    $gpuRows | ForEach-Object { Write-Output $_ }
    Write-Output '__GPUINFO_END__'
    Write-Output ('__IFACES__' + $ifaces)
    Write-Output '__ACTIVE_IFACE__all'
    Write-Output ('__RATES__{0}|{1}' -f [Math]::Round($rxRate), [Math]::Round($txRate))
    Write-Output '__IFACE_RATES_START__'
    $ifRates | ForEach-Object { Write-Output $_ }
    Write-Output '__IFACE_RATES_END__'
    Write-Output '__DISK_START__'
    $diskLines | ForEach-Object { Write-Output $_ }
    Write-Output '__DISK_END__'
    Write-Output '__FILESYSTEMS_START__'
    $fsLines | ForEach-Object { Write-Output $_ }
    Write-Output '__FILESYSTEMS_END__'
    Write-Output '__PROCS_START__'
    $procLines | ForEach-Object { Write-Output $_ }
    Write-Output '__PROCS_END__'
    Write-Output '__FILETERM_METRICS_BLOCK__'
}
"#,
    );
    let interval_ms = interval_seconds.saturating_mul(1_000);
    script
        .replace(
            "$nextEmitMs = [double]$sampleClock.ElapsedMilliseconds + 1000",
            &format!("$nextEmitMs = [double]$sampleClock.ElapsedMilliseconds + {interval_ms}"),
        )
        .replace(
            "$nextEmitMs += 1000",
            &format!("$nextEmitMs += {interval_ms}"),
        )
}

pub fn build_windows_streaming_metrics_exec_command(
    interval_seconds: u64,
) -> Result<String, String> {
    let script = build_windows_streaming_metrics_command(interval_seconds);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(script.as_bytes())
        .map_err(|error| error.to_string())?;
    let compressed = encoder.finish().map_err(|error| error.to_string())?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(compressed);
    let loader = format!(
        "$b=[Convert]::FromBase64String('{encoded}');$m=New-Object IO.MemoryStream(,$b);$g=New-Object IO.Compression.GzipStream($m,[IO.Compression.CompressionMode]::Decompress);$r=New-Object IO.StreamReader($g,[Text.Encoding]::UTF8);& ([scriptblock]::Create($r.ReadToEnd()))"
    );
    let command = format!(
        "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command \"{loader}\""
    );
    if command.len() >= 8000 {
        return Err(format!(
            "Windows streaming metrics command exceeds cmd.exe safe length: {}",
            command.len()
        ));
    }
    Ok(command)
}
