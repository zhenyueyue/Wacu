param([switch]$Archive, [int[]]$Sizes = @(0, 1, 2), [int[]]$Counts = @(0, 1, 3))
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$configPath = Join-Path $env:APPDATA 'Wacu\preferences.json'
$configBefore = if (Test-Path -LiteralPath $configPath) { (Get-FileHash -LiteralPath $configPath).Hash } else { $null }
foreach ($size in $Sizes) {
    foreach ($count in $Counts) {
        $start = [System.Diagnostics.ProcessStartInfo]::new((Join-Path $taskRoot 'target\debug\wacu.exe'))
        $start.WorkingDirectory = $taskRoot
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardError = $true
        $start.Environment['WACU_QA_VIEW'] = 'mini'
        $start.Environment['WACU_QA_ENTRY_MOTION'] = '1'
        $start.Environment['WACU_QA_SIZE'] = [string]$size
        $start.Environment['WACU_QA_COUNT'] = [string]$count
        $start.Environment['WACU_QA_EXIT'] = '1'
        $start.Environment['APPDATA'] = Join-Path $taskRoot "target\qa-entry-profile-$size-$count"
        $process = [System.Diagnostics.Process]::Start($start)
        if (!$process.WaitForExit(15000)) {
            $process.Kill()
            throw '入口动画检查超时'
        }
        $diagnostics = $process.StandardError.ReadToEnd()
        if ($process.ExitCode -ne 0 -or $diagnostics -notmatch 'entry_unfold_pixels_pass' -or $diagnostics -notmatch 'entry_reversal_disabled_pass') {
            throw "入口动画检查失败 ($size / $count, exit=$($process.ExitCode)): $diagnostics"
        }
        $process.Dispose()
        if ($Archive -and $size -eq 1 -and $count -eq 3) {
            foreach ($frame in @('mini-rest', 'mini-exit', 'open-early', 'open-mid', 'open-rest', 'open-settled', 'close-early', 'close-mid', 'mini-return-early', 'mini-return-mid', 'mini-return-rest', 'open-fx-40', 'open-fx-60', 'open-fx-100', 'open-fx-120', 'open-fx-160', 'open-fx-200', 'open-fx-260', 'open-fx-300', 'close-fx-20', 'close-fx-50', 'close-fx-110', 'close-fx-130', 'close-fx-170')) {
                Copy-Item -LiteralPath (Join-Path $taskRoot "target\qa-entry-$frame.png") -Destination (Join-Path $taskRoot "docs\screenshots\entry-$frame.png") -Force
            }
            foreach ($frame in @('mini-exit-40', 'mini-exit-60', 'mini-exit-100', 'mini-return-20', 'mini-return-50', 'mini-return-110', 'mini-return-130', 'mini-return-170')) {
                Copy-Item -LiteralPath (Join-Path $taskRoot "target\qa-entry-$frame.png") -Destination (Join-Path $taskRoot "docs\screenshots\entry-$frame.png") -Force
            }
        }
        Write-Output "动画通过: 尺寸 $size / 服务数 $count；原生帧展开与收起、快速反向、关闭动画立即切换"
    }
}
$configAfter = if (Test-Path -LiteralPath $configPath) { (Get-FileHash -LiteralPath $configPath).Hash } else { $null }
if ($configBefore -ne $configAfter) { throw '动画检查意外改变了用户配置' }
Write-Output '用户配置校验: 未改变'
