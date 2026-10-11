param([string[]]$Only = @('capsule', 'quick', 'detail', 'settings'))
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root 'target\debug\wacu.exe'
if (!(Test-Path -LiteralPath $exe)) { throw '请先运行 cargo build' }
$config = Join-Path $env:APPDATA 'Wacu\preferences.json'
$before = if (Test-Path -LiteralPath $config) { (Get-FileHash -LiteralPath $config).Hash } else { $null }
foreach ($view in $Only) {
    if ($view -notin @('capsule', 'quick', 'detail', 'settings')) { throw "未知检查界面: $view" }
    $source = Join-Path $root "target\wacu-qa-$view.png"
    if (Test-Path -LiteralPath $source) { Remove-Item -LiteralPath $source }
    $start = [System.Diagnostics.ProcessStartInfo]::new($exe)
    $start.WorkingDirectory = $root
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardError = $true
    $start.Environment['APPDATA'] = Join-Path $root 'target\qa-codex-profile'
    $start.Environment['WACU_QA_VIEW'] = $view
    $start.Environment['WACU_QA_SERVICE'] = '0'
    $start.Environment['WACU_QA_PAGE'] = '1'
    $start.Environment['WACU_QA_CODEX_LIVE'] = '1'
    $start.Environment['WACU_QA_EXIT'] = '1'
    $process = [System.Diagnostics.Process]::Start($start)
    if (!$process.WaitForExit(15000)) {
        $process.Kill()
        throw "Codex 界面检查超时: $view"
    }
    $diagnostics = $process.StandardError.ReadToEnd()
    if ($process.ExitCode -ne 0 -or $diagnostics -notmatch 'WACU_QA codex_live_model_pass' -or !(Test-Path -LiteralPath $source)) {
        throw "Codex 真实额度界面检查失败: $view $diagnostics"
    }
    $destination = Join-Path $root "target\qa-codex-$view.png"
    Copy-Item -LiteralPath $source -Destination $destination -Force
    Write-Output "PASS Codex $view -> $destination"
}
$after = if (Test-Path -LiteralPath $config) { (Get-FileHash -LiteralPath $config).Hash } else { $null }
if ($before -ne $after) { throw '用户偏好被界面检查修改' }
Write-Output 'PASS 用户偏好未修改；截图仅保存到 target，不提交账户额度'
