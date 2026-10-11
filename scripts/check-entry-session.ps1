$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $taskRoot 'target\debug\wacu.exe'
if (!(Test-Path -LiteralPath $exe)) { throw '请先运行 cargo build' }
$userConfig = Join-Path $env:APPDATA 'Wacu\preferences.json'
$before = if (Test-Path -LiteralPath $userConfig) { (Get-FileHash -LiteralPath $userConfig).Hash } else { $null }
$profile = Join-Path $taskRoot 'target\qa-entry-session-profile'
$config = Join-Path $profile 'Wacu\preferences.json'
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $config) | Out-Null
# Start with a legacy configuration: no entry_view and no window positions.
# Windows PowerShell 5 writes a BOM; Rust's JSON reader needs plain UTF-8.
[System.IO.File]::WriteAllText($config, '{"mode":"popup","capsule_size":1,"size_preset_version":2,"animations_enabled":true}', [System.Text.UTF8Encoding]::new($false))
$cases = @(
    @{ Initial = 'mini'; Action = 'verify'; Target = 'mini' },
    @{ Initial = 'mini'; Action = 'expand'; Target = 'capsule' },
    @{ Initial = 'capsule'; Action = 'verify-settings'; Target = 'capsule' },
    @{ Initial = 'capsule'; Action = 'collapse'; Target = 'mini' },
    @{ Initial = 'mini'; Action = 'verify'; Target = 'mini' },
    @{ Initial = 'mini'; Action = 'rapid-capsule'; Target = 'capsule' },
    @{ Initial = 'capsule'; Action = 'rapid-mini'; Target = 'mini' },
    @{ Initial = 'mini'; Action = 'verify'; Target = 'mini' },
    @{ Initial = 'mini'; Action = 'expand'; Target = 'capsule'; Animations = $false },
    @{ Initial = 'capsule'; Action = 'verify-settings'; Target = 'capsule' },
    @{ Initial = 'capsule'; Action = 'collapse'; Target = 'mini' },
    @{ Initial = 'mini'; Action = 'verify'; Target = 'mini' }
)
$miniPosition = $null
$capsulePosition = $null
foreach ($case in $cases) {
    if ($case.ContainsKey('Animations')) {
        $prefs = Get-Content -LiteralPath $config -Encoding UTF8 -Raw | ConvertFrom-Json
        $prefs.animations_enabled = $case.Animations
        [System.IO.File]::WriteAllText($config, ($prefs | ConvertTo-Json -Depth 3), [System.Text.UTF8Encoding]::new($false))
    }
    $start = [System.Diagnostics.ProcessStartInfo]::new($exe)
    $start.WorkingDirectory = $taskRoot
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardError = $true
    $start.Environment['APPDATA'] = $profile
    $start.Environment['WACU_QA_VIEW'] = 'entry-session'
    $start.Environment['WACU_QA_ENTRY_SESSION'] = $case.Action
    $start.Environment['WACU_QA_ENTRY_EXPECT'] = $case.Initial
    $process = [System.Diagnostics.Process]::Start($start)
    if (!$process.WaitForExit(15000)) {
        $process.Kill()
        throw ('窗口恢复检查超时: ' + $case.Action)
    }
    $diagnostics = $process.StandardError.ReadToEnd()
    if ($process.ExitCode -ne 0 -or $diagnostics -notmatch 'WACU_QA entry_session_pass') {
        throw ('窗口恢复检查失败: ' + $case.Action + ' ' + $diagnostics)
    }
    $process.Dispose()
    $saved = Get-Content -LiteralPath $config -Encoding UTF8 -Raw | ConvertFrom-Json
    if ($saved.entry_view -ne $case.Target) { throw ('退出后窗口形态不匹配: ' + $case.Action) }
    if ($null -ne $saved.mini_position) {
        $position = $saved.mini_position -join ','
        if ($null -ne $miniPosition -and $miniPosition -ne $position) { throw 'Mini 坐标被覆盖' }
        $miniPosition = $position
    }
    if ($null -ne $saved.saved_position) {
        $position = $saved.saved_position -join ','
        if ($null -ne $capsulePosition -and $capsulePosition -ne $position) { throw '胶囊坐标被覆盖' }
        $capsulePosition = $position
    }
    Write-Output ('退出并重新启动检查通过: ' + $case.Initial + ' / ' + $case.Action + ' → ' + $saved.entry_view)
}
if ($null -eq $miniPosition -or $null -eq $capsulePosition) { throw '缺少独立窗口坐标' }
$after = if (Test-Path -LiteralPath $userConfig) { (Get-FileHash -LiteralPath $userConfig).Hash } else { $null }
if ($before -ne $after) { throw '窗口恢复检查意外改变用户配置' }
Write-Output '用户配置校验: 未改变'
