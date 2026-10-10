$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$configPath = Join-Path $env:APPDATA 'Wacu\preferences.json'
$configBefore = if (Test-Path -LiteralPath $configPath) { (Get-FileHash -LiteralPath $configPath).Hash } else { $null }
foreach ($mode in @('popup', 'pinned')) {
    $start = [System.Diagnostics.ProcessStartInfo]::new((Join-Path $taskRoot 'target\debug\wacu.exe'))
    $start.WorkingDirectory = $taskRoot
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardError = $true
    $start.Environment['WACU_QA_VIEW'] = 'capsule'
    $start.Environment['WACU_QA_STARTUP'] = $mode
    $start.Environment['WACU_QA_EXIT'] = '1'
    $start.Environment['APPDATA'] = Join-Path $taskRoot 'target\qa-startup-profile'
    $process = [System.Diagnostics.Process]::Start($start)
    if (!$process.WaitForExit(15000)) {
        $process.Kill()
        throw ('启动显隐检查超时: ' + $mode)
    }
    $diagnostics = $process.StandardError.ReadToEnd()
    if ($process.ExitCode -ne 0 -or $diagnostics -notmatch 'WACU_QA startup_shown' -or $diagnostics -notmatch 'WACU_QA startup_pass') {
        throw ('启动显隐检查失败: ' + $mode + ' ' + $diagnostics)
    }
    $process.Dispose()
    Write-Output ('启动显隐检查通过: ' + $mode)
}
$configAfter = if (Test-Path -LiteralPath $configPath) { (Get-FileHash -LiteralPath $configPath).Hash } else { $null }
if ($configBefore -ne $configAfter) { throw '启动检查意外改变用户配置' }
Write-Output '用户配置校验: 未改变'
