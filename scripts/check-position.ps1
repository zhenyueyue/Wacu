$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$configPath = Join-Path $env:APPDATA 'Wacu\preferences.json'
$configBefore = if (Test-Path -LiteralPath $configPath) { (Get-FileHash -LiteralPath $configPath).Hash } else { $null }
$start = [System.Diagnostics.ProcessStartInfo]::new((Join-Path $taskRoot 'target\debug\wacu.exe'))
$start.WorkingDirectory = $taskRoot
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardError = $true
$start.Environment['WACU_QA_VIEW'] = 'capsule'
$start.Environment['WACU_QA_POSITION'] = '1'
$start.Environment['WACU_QA_EXIT'] = '1'
$start.Environment['APPDATA'] = Join-Path $taskRoot 'target\qa-position-profile'
$process = [System.Diagnostics.Process]::Start($start)
if (!$process.WaitForExit(15000)) {
    $process.Kill()
    throw '模式位置检查超时'
}
$diagnostics = $process.StandardError.ReadToEnd()
if ($process.ExitCode -ne 0 -or $diagnostics -notmatch 'WACU_QA position_pass') {
    throw ('模式位置检查失败: ' + $diagnostics)
}
$process.Dispose()
$configAfter = if (Test-Path -LiteralPath $configPath) { (Get-FileHash -LiteralPath $configPath).Hash } else { $null }
if ($configBefore -ne $configAfter) { throw '模式检查意外改变用户配置' }
Write-Output '模式位置检查通过: 拖动、弹出、常驻、贴边隐藏、左侧位置与保存；用户配置未改变'
