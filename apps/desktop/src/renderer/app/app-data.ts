import type {
  ConnectionProfile,
  CreateProfileInput,
  LocalFileItem,
  WorkspaceSnapshot
} from '@termdock/core'

export const emptyState: WorkspaceSnapshot = {
  profiles: [],
  folders: [],
  commandFolders: [],
  commandTemplates: [],
  tabs: [],
  activeTabId: null,
  transfers: [],
  sessions: {}
}

export const localPreviewFiles: LocalFileItem[] = [
  { path: '/Users/stoffel', name: '..', type: 'folder', modified: '2026/05/15 18:44', size: '-' },
  { path: '/Users/stoffel/Downloads', name: 'Downloads', type: 'folder', modified: '2026/05/15 18:44', size: '-' },
  { path: '/Users/stoffel/Desktop', name: 'Desktop', type: 'folder', modified: '2026/05/15 18:32', size: '-' },
  { path: '/Users/stoffel/release.tar.gz', name: 'release.tar.gz', type: 'file', modified: '2026/05/15 17:18', size: '742 MB' },
  { path: '/Users/stoffel/backup.sql.gz', name: 'backup.sql.gz', type: 'file', modified: '2026/05/15 16:02', size: '1.1 GB' }
]

export const previewLocalPath = '/Users/stoffel'

export const previewState: WorkspaceSnapshot = {
  profiles: [
    {
      id: 'preview-profile-ssh',
      type: 'ssh',
      name: 'SynologyNAS',
      host: '114.66.28.185',
      port: 22,
      username: 'Stoffel',
      authType: 'privateKey',
      privateKeyPath: '~/.ssh/id_ed25519',
      group: '默认',
      sftpEnabled: true,
      remotePath: '/'
    },
    {
      id: 'preview-profile-ftp',
      type: 'ftp',
      name: 'archive-ftp',
      host: 'ftp.example.net',
      port: 21,
      username: 'deploy',
      secure: false,
      group: 'FTP',
      remotePath: '/incoming'
    }
  ],
  folders: [],
  commandFolders: [
    { id: 'preview-cmd-folder-default', type: 'command-folder', name: '默认分类', order: 1000 },
    { id: 'preview-cmd-folder-system', type: 'command-folder', name: '系统诊断', order: 2000 }
  ],
  commandTemplates: [
    {
      id: 'preview-cmd-disk',
      type: 'command-template',
      name: '磁盘占用',
      parentId: 'preview-cmd-folder-default',
      order: 1000,
      command: 'df -h',
      description: '查看当前主机磁盘使用情况',
      appendCarriageReturn: true
    },
    {
      id: 'preview-cmd-tail',
      type: 'command-template',
      name: '日志追踪',
      parentId: 'preview-cmd-folder-system',
      order: 2000,
      command: 'tail -f [p#1]',
      description: '输入日志路径后实时查看输出',
      appendCarriageReturn: true
    }
  ],
  tabs: [
    {
      id: 'preview-tab-ssh',
      sessionType: 'ssh',
      profileId: 'preview-profile-ssh',
      title: '123',
      layout: 'terminal-file',
      status: 'connected'
    }
  ],
  activeTabId: 'preview-tab-ssh',
  transfers: [],
  sessions: {
    'preview-tab-ssh': {
      profileId: 'preview-profile-ssh',
      summary: 'Connected to 192.168.3.197:22',
      terminalTranscript:
        'Linux fnOSNAS-CN 6.18.18-trim #473 SMP PREEMPT_DYNAMIC Thu Apr  9 09:34:02 UTC 2026 x86_64\r\nLast login: Fri May 15 21:57:26 2026 from 127.0.0.1\r\nCould not chdir to home directory /home/Stoffel: No such file or directory\r\nStoffel@fnOSNAS-CN:~$ ',
      remotePath: '/',
      remoteFiles: [
        { path: '/boot', name: 'boot', type: 'folder', modified: '2026-05-11 17:46', size: '-', permission: 'drwxr-xr-x', ownerGroup: '0/0' },
        { path: '/dev', name: 'dev', type: 'folder', modified: '2026-05-15 07:20', size: '-', permission: 'drwxr-xr-x', ownerGroup: '0/0' },
        { path: '/etc', name: 'etc', type: 'folder', modified: '2026-05-11 17:46', size: '-', permission: 'drwxr-xr-x', ownerGroup: '0/0' },
        { path: '/home', name: 'home', type: 'folder', modified: '2024-08-01 16:06', size: '-', permission: 'drwxr-xr-x', ownerGroup: '0/0' },
        { path: '/run', name: 'run', type: 'folder', modified: '2026-05-15 07:20', size: '-', permission: 'drwxr-xr-x', ownerGroup: '0/0' }
      ],
      connected: true,
      systemMetrics: {
        ip: '192.168.3.197',
        uptime: '4 天',
        load: '0.44, 0.66, 0.62',
        identity: {
          osName: 'fnOS 26.04',
          kernelName: 'Linux',
          kernelVersion: '6.18.18-trim',
          architecture: 'x86_64',
          hostname: 'fnOSNAS-CN'
        },
        cpuPercent: 10,
        cpuUsage: {
          user: 6.2,
          system: 3.1,
          nice: 0.2,
          idle: 85.7,
          ioWait: 0,
          irq: 0.3,
          softIrq: 0.8,
          steal: 3.7
        },
        cpuInfoRows: [
          { model: 'Intel(R) Core(TM) i5-12600K', cores: 10, frequencyMHz: '3687.000', cache: '20480 KB', bogomips: '7374.00' }
        ],
        memoryPercent: 68,
        memoryUsage: '7.9G/11.6G',
        memoryBreakdown: {
          total: '11.6G',
          used: '7.9G',
          available: '3.7G',
          percent: 68
        },
        swapPercent: 7,
        swapUsage: '290M/4.0G',
        swapBreakdown: {
          total: '4.0G',
          used: '290M',
          available: '3.7G',
          percent: 7
        },
        diskRows: [
          { path: '/dev', usage: '5.8G/5.8G' },
          { path: '/run', usage: '1.1G/1.2G' },
          { path: '/', usage: '44G/63G' },
          { path: '/dev/shm', usage: '5.9G/5.9G' },
          { path: '/run/lock', usage: '5.0M/5.0M' }
        ],
        fileSystemRows: [
          { name: '/dev/nvme0n1p2', size: '63G', used: '44G', usagePercent: '70%', available: '19G', mountPoint: '/' },
          { name: 'tmpfs', size: '5.9G', used: '144K', usagePercent: '0%', available: '5.9G', mountPoint: '/dev/shm' }
        ],
        networkInterfaces: ['enp3s0-ovs'],
        activeNetworkInterface: 'enp3s0-ovs',
        networkRates: { tx: '540B', rx: '233B' },
        networkSamples: Array.from({ length: 18 }, (_, index) => ({
          tx: [5, 11, 8, 14, 4, 9, 2, 12, 10, 6, 15, 7, 3, 8, 11, 4, 6, 9][index],
          rx: [10, 19, 13, 22, 6, 24, 8, 15, 18, 12, 20, 9, 5, 13, 16, 8, 11, 14][index]
        })),
        networkInterfaceRows: [
          { name: 'enp3s0-ovs', txTotal: '1.4 TB', rxTotal: '692 GB', txRate: '540B', rxRate: '233B' }
        ],
        topProcesses: [
          { memory: '3171.4M', cpu: '3.0', command: 'python', elapsedSeconds: 18600 },
          { memory: '1309.4M', cpu: '1.0', command: 'next-server', elapsedSeconds: 9700 },
          { memory: '536.1M', cpu: '0.6', command: 'python3', elapsedSeconds: 8400 },
          { memory: '349.7M', cpu: '0.5', command: 'trim-photos', elapsedSeconds: 4100 }
        ]
      }
    }
  }
}

export const defaultForm: CreateProfileInput = {
  type: 'ssh',
  name: '',
  host: '',
  port: 22,
  username: '',
  group: '默认',
  remotePath: '/',
  note: '',
  password: '',
  privateKeyPath: '',
  passphrase: '',
  authType: 'password',
  encoding: 'UTF-8',
  backspaceKey: 'ASCII',
  deleteKey: 'VT220',
  enableExecChannel: true,
  secure: false
}

export function profileToForm(profile: ConnectionProfile): CreateProfileInput {
  return {
    type: profile.type,
    name: profile.name,
    host: profile.host,
    port: profile.port,
    username: profile.username,
    group: profile.group,
    remotePath: profile.remotePath,
    note: profile.note ?? '',
    password: profile.password ?? '',
    authType: profile.type === 'ssh'
      ? (profile.authType === 'system' ? 'password' : profile.authType)
      : 'password',
    privateKeyPath: profile.type === 'ssh' ? profile.privateKeyPath ?? '' : '',
    passphrase: profile.type === 'ssh' ? profile.passphrase ?? '' : '',
    encoding: profile.type === 'ssh' ? profile.encoding ?? 'UTF-8' : 'UTF-8',
    backspaceKey: profile.type === 'ssh' ? profile.backspaceKey ?? 'ASCII' : 'ASCII',
    deleteKey: profile.type === 'ssh' ? profile.deleteKey ?? 'VT220' : 'VT220',
    enableExecChannel: profile.type === 'ssh' ? profile.enableExecChannel ?? true : true,
    secure: profile.type === 'ftp' ? profile.secure : false
  }
}
