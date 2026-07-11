import assert from 'node:assert/strict'
import test from 'node:test'
import { parseSystemMetrics } from '../../src/main/services/sessions/system-metrics/parser.ts'

test('parses normalized Linux metrics without localized uptime text', () => {
  const metrics = parseSystemMetrics(
    [
      '__PLATFORM__linux',
      '__OS__Ubuntu 24.04 LTS',
      '__KERNEL_NAME__Linux',
      '__KERNEL_VERSION__6.8.0',
      '__ARCH__x86_64',
      '__HOSTNAME__server-a',
      '__IP__10.0.0.10',
      '__UPTIME__',
      '__UPTIME_SECONDS__90061',
      '__LOAD__0.10, 0.20, 0.30',
      '__CPU__12',
      '__CPU_USAGE__3|4|0|88|1|0|0|0',
      '__MEM__1024|4096|25|512|256|128',
      '__MEM_BYTES__1073741824|4294967296|3221225472|25|536870912|268435456|134217728',
      '__SWAP__128|1024|12',
      '__SWAP_BYTES__134217728|1073741824|939524096|12',
      '__CPUINFO_START__',
      'AMD EPYC|8|3000|-|-',
      '__CPUINFO_END__',
      '__GPUINFO_START__',
      '__GPUINFO_END__',
      '__IFACES__eth0',
      '__ACTIVE_IFACE__eth0',
      '__RATES__1024|2048',
      '__IFACE_RATES_START__',
      'eth0|4096|8192|1024|2048',
      '__IFACE_RATES_END__',
      '__DISK_START__',
      '/|80G/100G',
      '__DISK_END__',
      '__FILESYSTEMS_START__',
      '/dev/sda1|100G|20G|20%|80G|/',
      '__FILESYSTEMS_END__',
      '__PROCS_START__',
      '32.0M|1.5|60|node',
      '__PROCS_END__'
    ].join('\n')
  )

  assert.equal(metrics.platform, 'linux')
  assert.equal(metrics.uptime, '-')
  assert.equal(metrics.uptimeSeconds, 90061)
  assert.equal(metrics.memoryRaw?.totalBytes, 4294967296)
  assert.equal(metrics.memoryBreakdown.available, '3.0G')
  assert.equal(metrics.networkRawByInterface?.eth0?.rxBytesPerSecond, 1024)
  assert.equal(metrics.networkRatesByInterface?.eth0?.tx, '2K')
  assert.equal(metrics.diskRows[0]?.usage, '80.0 GB/100.0 GB')
  assert.equal(metrics.fileSystemRows[0]?.size, '100.0 GB')
  assert.equal(metrics.fileSystemRows[0]?.available, '80.0 GB')
})

test('humanizes POSIX df -k storage values while preserving Windows values', () => {
  const posixMetrics = parseSystemMetrics(
    [
      '__PLATFORM__linux',
      '__DISK_START__',
      '/dev|8153252K/8153252K',
      '/|249799032K/292811652K',
      '__DISK_END__',
      '__FILESYSTEMS_START__',
      '/dev/sda1|292811652K|43012620K|15%|249799032K|/',
      '__FILESYSTEMS_END__'
    ].join('\n')
  )
  const windowsMetrics = parseSystemMetrics(
    ['__PLATFORM__windows', '__DISK_START__', 'C:|53.6 GB/400.1 GB', '__DISK_END__'].join('\n')
  )

  assert.equal(posixMetrics.diskRows[0]?.usage, '7.8 GB/7.8 GB')
  assert.equal(posixMetrics.diskRows[1]?.usage, '238.2 GB/279.2 GB')
  assert.equal(posixMetrics.fileSystemRows[0]?.size, '279.2 GB')
  assert.equal(posixMetrics.fileSystemRows[0]?.used, '41.0 GB')
  assert.equal(posixMetrics.fileSystemRows[0]?.available, '238.2 GB')
  assert.equal(windowsMetrics.diskRows[0]?.usage, '53.6 GB/400.1 GB')
})

test('parses BusyBox metrics with missing optional collectors', () => {
  const metrics = parseSystemMetrics(
    [
      '__PLATFORM__busybox',
      '__OS__OpenWrt',
      '__KERNEL_NAME__Linux',
      '__KERNEL_VERSION__5.15',
      '__ARCH__mips',
      '__HOSTNAME__router',
      '__IP__192.168.1.1',
      '__UPTIME__',
      '__UPTIME_SECONDS__60',
      '__LOAD__0.00, 0.01, 0.05',
      '__CPU__1',
      '__CPU_USAGE__0|1|0|99|0|0|0|0',
      '__MEM__64|128|50|0|0|0',
      '__MEM_BYTES__67108864|134217728|67108864|50|0|0|0',
      '__SWAP__0|0|0',
      '__SWAP_BYTES__0|0|0|0',
      '__CPUINFO_START__',
      '__CPUINFO_END__',
      '__GPUINFO_START__',
      '__GPUINFO_END__',
      '__IFACES__br-lan',
      '__ACTIVE_IFACE__br-lan',
      '__RATES__0|0',
      '__IFACE_RATES_START__',
      '__IFACE_RATES_END__',
      '__DISK_START__',
      '__DISK_END__',
      '__FILESYSTEMS_START__',
      '__FILESYSTEMS_END__',
      '__PROCS_START__',
      '__PROCS_END__'
    ].join('\n')
  )

  assert.equal(metrics.platform, 'busybox')
  assert.equal(metrics.memoryUsage, '64M/128M')
  assert.deepEqual(metrics.gpuInfoRows, [])
})

test('parses Windows metrics from PowerShell collector markers', () => {
  const metrics = parseSystemMetrics(
    [
      '__PLATFORM__windows',
      '__OS__Microsoft Windows Server 2022',
      '__KERNEL_NAME__Windows',
      '__KERNEL_VERSION__10.0.20348',
      '__ARCH__64-bit',
      '__HOSTNAME__WIN-SRV',
      '__IP__10.0.0.20',
      '__UPTIME__',
      '__UPTIME_SECONDS__3600',
      '__LOAD__1.25',
      '__LOAD_UNIT__busy-logical-processors',
      '__CPU__33',
      '__CPU_USAGE__0|33|0|67|0|0|0|0',
      '__MEM__2048|8192|25|0|0|0',
      '__MEM_BYTES__2147483648|8589934592|6442450944|25|0|0|0',
      '__SWAP__0|0|0',
      '__SWAP_BYTES__0|0|0|0',
      '__CPUINFO_START__',
      'Intel Xeon|4|3200|-|-',
      '__CPUINFO_END__',
      '__GPUINFO_START__',
      'Microsoft Basic Display Adapter|Microsoft|10.0|-',
      '__GPUINFO_END__',
      '__IFACES__Ethernet',
      '__ACTIVE_IFACE__Ethernet',
      '__RATES__512|256',
      '__IFACE_RATES_START__',
      'Ethernet|1000|2000|512|256',
      '__IFACE_RATES_END__',
      '__DISK_START__',
      'C:|20 GB/60 GB',
      '__DISK_END__',
      '__FILESYSTEMS_START__',
      'C:|60 GB|40 GB|66%|20 GB|C:',
      '__FILESYSTEMS_END__',
      '__PROCS_START__',
      '100.0M|0.0|120|sshd',
      '__PROCS_END__'
    ].join('\n')
  )

  assert.equal(metrics.platform, 'windows')
  assert.equal(metrics.identity.kernelName, 'Windows')
  assert.equal(metrics.load, '1.25')
  assert.equal(metrics.loadUnit, 'busy-logical-processors')
  assert.equal(metrics.swapRaw?.totalBytes, 0)
  assert.equal(metrics.networkRawByInterface?.Ethernet?.txBytes, 2000)
})

test('normalizes CRLF Windows metrics before parsing marker lines and blocks', () => {
  const metrics = parseSystemMetrics(
    [
      '__PLATFORM__windows',
      '__OS__Microsoft Windows Server 2022 ',
      '__KERNEL_NAME__Windows',
      '__KERNEL_VERSION__10.0.20348',
      '__ARCH__64-bit',
      '__HOSTNAME__WIN-CRLF',
      '__IP__10.0.0.21',
      '__UPTIME_SECONDS__7200',
      '__CPU__18',
      '__CPU_USAGE__0|18|0|82|0|0|0|0',
      '__MEM__1024|4096|25|0|0|0',
      '__MEM_BYTES__1073741824|4294967296|3221225472|25|0|0|0',
      '__SWAP__0|0|0',
      '__SWAP_BYTES__0|0|0|0',
      '__CPUINFO_START__',
      ' Intel Xeon|8|2800|-|- ',
      '__CPUINFO_END__',
      '__GPUINFO_START__',
      '__GPUINFO_END__',
      '__IFACES__Ethernet',
      '__RATES__128|64',
      '__IFACE_RATES_START__',
      ' Ethernet|1000|2000|128|64 ',
      '__IFACE_RATES_END__',
      '__DISK_START__',
      ' C:|20 GB/60 GB ',
      '__DISK_END__',
      '__FILESYSTEMS_START__',
      ' C:|60 GB|40 GB|66%|20 GB|C: ',
      '__FILESYSTEMS_END__',
      '__PROCS_START__',
      ' 50.0M|0.0|30|sshd ',
      '__PROCS_END__'
    ].join('\r\n'),
    'linux'
  )

  assert.equal(metrics.platform, 'windows')
  assert.equal(metrics.identity.osName, 'Microsoft Windows Server 2022')
  assert.equal(metrics.identity.hostname, 'WIN-CRLF')
  assert.equal(metrics.cpuInfoRows[0]?.model, 'Intel Xeon')
  assert.equal(metrics.networkRawByInterface?.Ethernet?.rxBytesPerSecond, 128)
  assert.equal(metrics.diskRows[0]?.path, 'C:')
  assert.equal(metrics.topProcesses[0]?.command, 'sshd')
})
