param([switch]$Live, [string[]]$Only)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root 'target\debug\wacu.exe'
if (!(Test-Path -LiteralPath $exe)) { throw '请先运行 cargo build' }
$config = Join-Path $env:APPDATA 'Wacu\preferences.json'
$before = if (Test-Path -LiteralPath $config) { (Get-FileHash -LiteralPath $config).Hash } else { $null }
$cases = @('capsule', 'quick', 'detail', 'settings') | ForEach-Object { @{ Name = $_; View = $_; Fixture = 'ok' } }
if (!$Live) {
    foreach ($fixture in @('unused', 'exhausted')) {
        foreach ($view in @('capsule', 'quick', 'detail')) {
            $cases += @{ Name = "$fixture-$view"; View = $view; Fixture = $fixture }
        }
    }
    $cases += @{ Name = 'unauthorized'; View = 'quick'; Fixture = 'unauthorized' }
    $cases += @{ Name = 'rate-limited'; View = 'quick'; Fixture = 'rate-limited' }
}
foreach ($case in $cases) {
    if ($Only -and $case.Name -notin $Only) { continue }
    $source = Join-Path $root ('target\wacu-qa-' + $case.View + '.png')
    if (Test-Path -LiteralPath $source) { Remove-Item -LiteralPath $source }
    $start = [System.Diagnostics.ProcessStartInfo]::new($exe)
    $start.WorkingDirectory = $root
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardError = $true
    $start.Environment['APPDATA'] = Join-Path $root 'target\qa-claude-profile'
    $start.Environment['WACU_QA_VIEW'] = $case.View
    $start.Environment['WACU_QA_SERVICE'] = '1'
    $start.Environment['WACU_QA_PAGE'] = '1'
    $start.Environment['WACU_QA_EXIT'] = '1'
    [void]$start.Environment.Remove('WACU_QA_CODEX_LIVE')
    [void]$start.Environment.Remove('WACU_QA_CLAUDE_LIVE')
    [void]$start.Environment.Remove('WACU_QA_CLAUDE_FIXTURE')
    if ($Live) { $start.Environment['WACU_QA_CLAUDE_LIVE'] = '1' }
    else { $start.Environment['WACU_QA_CLAUDE_FIXTURE'] = $case.Fixture }
    $process = [System.Diagnostics.Process]::Start($start)
    $timeout = if ($Live) { 55000 } else { 15000 }
    if (!$process.WaitForExit($timeout)) { $process.Kill(); throw ('Claude 界面检查超时: ' + $case.Name) }
    $diagnostics = $process.StandardError.ReadToEnd()
    if ($process.ExitCode -ne 0 -or $diagnostics -notmatch 'WACU_QA claude_model_pass' -or !(Test-Path -LiteralPath $source)) {
        throw ('Claude 界面检查失败: ' + $case.Name + ' ' + $diagnostics)
    }
    $kind = if ($Live) { 'live' } else { 'fixture' }
    $destination = Join-Path $root ('target\qa-claude-' + $kind + '-' + $case.Name + '.png')
    Copy-Item -LiteralPath $source -Destination $destination -Force
    Write-Output ('PASS Claude ' + $kind + ' ' + $case.Name)
}
$after = if (Test-Path -LiteralPath $config) { (Get-FileHash -LiteralPath $config).Hash } else { $null }
if ($before -ne $after) { throw '用户偏好被界面检查修改' }
Write-Output 'PASS 用户偏好未修改；截图仅在 target'
