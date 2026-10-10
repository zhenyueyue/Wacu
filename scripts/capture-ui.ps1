param([switch]$Archive)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root 'target\debug\wacu.exe'
if (!(Test-Path -LiteralPath $exe)) { throw '请先运行 cargo build' }
$configPath = Join-Path $env:APPDATA 'Wacu\preferences.json'
$configBefore = if (Test-Path -LiteralPath $configPath) { (Get-FileHash -LiteralPath $configPath).Hash } else { $null }
$cases = @(
    @{ View = 'capsule'; Name = 'capsule'; Count = 3 },
    @{ View = 'capsule'; Name = 'capsule-compact'; Count = 3; Size = 0 },
    @{ View = 'capsule'; Name = 'capsule-spacious'; Count = 3; Size = 2 },
    @{ View = 'capsule'; Name = 'capsule-selected'; Count = 3; Selected = 1 },
    @{ View = 'capsule'; Name = 'capsule-empty'; Count = 0 },
    @{ View = 'capsule'; Name = 'capsule-one'; Count = 1 },
    @{ View = 'capsule'; Name = 'capsule-two'; Count = 2 },
    @{ View = 'quick'; Name = 'quick-detail'; Count = 3 },
    @{ View = 'detail'; Name = 'full-detail'; Count = 3; Period = 0 },
    @{ View = 'detail'; Name = 'detail-week'; Count = 3; Period = 1 },
    @{ View = 'detail'; Name = 'detail-month'; Count = 3; Period = 2 },
    @{ View = 'detail'; Name = 'detail-all'; Count = 3; Period = 3 }
)
$pages = @('general', 'services', 'appearance', 'behavior', 'about')
for ($page = 0; $page -lt $pages.Count; $page++) {
    $cases += @{ View = 'settings'; Name = 'settings-' + $pages[$page]; Count = 3; Page = $page }
}
foreach ($case in $cases) {
    $source = Join-Path $root ('target\wacu-qa-' + $case.View + '.png')
    if (Test-Path -LiteralPath $source) { Remove-Item -LiteralPath $source }
    $start = [System.Diagnostics.ProcessStartInfo]::new($exe)
    $start.WorkingDirectory = $root
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardError = $true
    $start.Environment['WACU_QA_VIEW'] = $case.View
    $start.Environment['WACU_QA_COUNT'] = [string]$case.Count
    $start.Environment['WACU_QA_PAGE'] = [string]$case.Page
    $start.Environment['WACU_QA_PERIOD'] = [string]$case.Period
    $start.Environment['WACU_QA_SELECTED'] = [string]$case.Selected
    $start.Environment['WACU_QA_SIZE'] = [string]$case.Size
    $start.Environment['WACU_QA_EXIT'] = '1'
    $process = [System.Diagnostics.Process]::Start($start)
    if (!$process.WaitForExit(15000)) {
        $process.Kill()
        throw ('预览超时: ' + $case.Name)
    }
    $diagnostics = $process.StandardError.ReadToEnd()
    if ($process.ExitCode -ne 0 -or !(Test-Path -LiteralPath $source)) {
        throw ('预览失败: ' + $case.Name + ' ' + $diagnostics)
    }
    $destination = Join-Path $root ('target\qa-' + $case.Name + '.png')
    Copy-Item -LiteralPath $source -Destination $destination -Force
    if ($Archive) {
        Copy-Item -LiteralPath $source -Destination (Join-Path $root ('docs\screenshots\' + $case.Name + '.png')) -Force
    }
    Write-Output ('已生成: ' + $case.Name)
    $process.Dispose()
}
$configAfter = if (Test-Path -LiteralPath $configPath) { (Get-FileHash -LiteralPath $configPath).Hash } else { $null }
if ($configBefore -ne $configAfter) { throw '预览检查意外改变了用户配置' }
Write-Output '用户配置校验: 未改变'
