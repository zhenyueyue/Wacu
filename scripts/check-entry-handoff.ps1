param([switch]$Archive, [int[]]$Counts = @(0, 1, 2, 3), [int[]]$Directions = @(0, 1, 2))
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$configPath = Join-Path $env:APPDATA 'Wacu\preferences.json'
$before = if (Test-Path -LiteralPath $configPath) { (Get-FileHash -LiteralPath $configPath).Hash } else { $null }
foreach ($count in $Counts) {
    foreach ($direction in $Directions) {
        $start = [System.Diagnostics.ProcessStartInfo]::new((Join-Path $taskRoot 'target\debug\wacu.exe'))
        $start.WorkingDirectory = $taskRoot
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardError = $true
        $start.Environment['WACU_QA_VIEW'] = 'mini'
        $start.Environment['WACU_QA_ENTRY_HANDOFF'] = [string]$direction
        $start.Environment['WACU_QA_COUNT'] = [string]$count
        $start.Environment['WACU_QA_SIZE'] = '1'
        $start.Environment['WACU_QA_EXIT'] = '1'
        $start.Environment['APPDATA'] = Join-Path $taskRoot "target\qa-handoff-profile-$count-$direction"
        $process = [System.Diagnostics.Process]::Start($start)
        if (!$process.WaitForExit(30000)) { $process.Kill(); throw '跨位置动画检查超时' }
        $diagnostics = $process.StandardError.ReadToEnd()
        if ($process.ExitCode -ne 0 -or $diagnostics -notmatch 'entry_handoff_pass') {
            throw "跨位置动画失败 ($count / $direction, exit=$($process.ExitCode)): $diagnostics"
        }
        $process.Dispose()
        if ($Archive -and $direction -eq 1) {
            $destination = Join-Path $taskRoot "target\handoff-frames-$count"
            New-Item -ItemType Directory -Force -Path $destination | Out-Null
            $scenes = Get-Content (Join-Path $taskRoot 'target\qa-entry-handoff-scenes.json') -Encoding UTF8 -Raw | ConvertFrom-Json
            Copy-Item -LiteralPath (Join-Path $taskRoot 'target\qa-entry-handoff-scenes.json') -Destination (Join-Path $destination 'scenes.json') -Force
            foreach ($scene in $scenes) { foreach ($window in $scene.windows) {
                Copy-Item -LiteralPath (Join-Path $taskRoot "target\qa-entry-$($window.name).png") -Destination (Join-Path $destination "$($window.name).png") -Force
            } }
        }
        Write-Output "跨位置通过: 服务数 $count / 路径 $direction；两端同步帧、独立坐标、任务栏排除、快速反向与动画开关"
    }
}
$after = if (Test-Path -LiteralPath $configPath) { (Get-FileHash -LiteralPath $configPath).Hash } else { $null }
if ($before -ne $after) { throw '跨位置检查改变了用户配置' }
Write-Output '用户配置校验: 未改变'
