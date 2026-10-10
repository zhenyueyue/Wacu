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
    @{ View = 'capsule'; Name = 'capsule-logo-drag'; Count = 3; Size = 1; LogoDrag = 1 },
    @{ View = 'capsule'; Name = 'capsule-gear-pressed'; Count = 3; Size = 1; GearPressed = 1 },
    @{ View = 'capsule'; Name = 'capsule-service-pressed'; Count = 3; Size = 1; ServicePressed = 0 },
    @{ View = 'capsule'; Name = 'capsule-selected'; Count = 3; Selected = 1 },
    @{ View = 'capsule'; Name = 'capsule-empty'; Count = 0 },
    @{ View = 'capsule'; Name = 'capsule-one'; Count = 1 },
    @{ View = 'capsule'; Name = 'capsule-two'; Count = 2 },
    @{ View = 'quick'; Name = 'quick-detail'; Count = 3; DragDetail = 1 },
    @{ View = 'quick'; Name = 'detail-animation'; Count = 3; DetailFade = 1 },
    @{ View = 'detail'; Name = 'full-detail'; Count = 3; Period = 0 },
    @{ View = 'detail'; Name = 'detail-week'; Count = 3; Period = 1 },
    @{ View = 'detail'; Name = 'detail-month'; Count = 3; Period = 2 },
    @{ View = 'detail'; Name = 'detail-all'; Count = 3; Period = 3 }
)
$pages = @('general', 'services', 'appearance', 'behavior', 'about')
for ($page = 0; $page -lt $pages.Count; $page++) {
    $cases += @{ View = 'settings'; Name = 'settings-' + $pages[$page]; Count = 3; Page = $page }
}
$cases += @{ View = 'settings'; Name = 'settings-input-check'; Count = 3; Page = 0; Input = 1 }
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
    if ($case.LogoDrag) { $start.Environment['WACU_QA_LOGO_DRAG'] = '1' }
    if ($case.GearPressed) { $start.Environment['WACU_QA_SETTINGS_PRESS'] = '1' }
    if ($case.ContainsKey('ServicePressed')) { $start.Environment['WACU_QA_SERVICE_PRESS'] = [string]$case.ServicePressed }
    if ($case.DragDetail) { $start.Environment['WACU_QA_DRAG_DETAIL'] = '1' }
    if ($case.DetailFade) { $start.Environment['WACU_QA_DETAIL_FADE'] = '1' }
    $start.Environment['WACU_QA_EXIT'] = '1'
    [void]$start.Environment.Remove('WACU_QA_INPUT')
    if ($case.Input) {
        $start.Environment['WACU_QA_INPUT'] = '1'
        $start.Environment['APPDATA'] = Join-Path $root 'target\qa-input-profile'
    }
    $process = [System.Diagnostics.Process]::Start($start)
    if (!$process.WaitForExit(15000)) {
        $process.Kill()
        throw ('预览超时: ' + $case.Name)
    }
    $diagnostics = $process.StandardError.ReadToEnd()
    if ($process.ExitCode -ne 0 -or !(Test-Path -LiteralPath $source)) {
        throw ('预览失败: ' + $case.Name + ' ' + $diagnostics)
    }
    if ($case.Input -and $diagnostics -notmatch 'WACU_QA input_pass') { throw ('输入检查未通过: ' + $diagnostics) }
    if ($case.DragDetail -and $diagnostics -notmatch 'WACU_QA detail_hide_restore_pass') {
        throw ('详情拖动恢复检查未通过: ' + $diagnostics)
    }
    if ($case.DetailFade -and ($diagnostics -notmatch 'WACU_QA detail_fade_pass' -or $diagnostics -notmatch 'WACU_QA detail_alpha_pass')) {
        throw ('详情淡入淡出或实际透明度帧检查未通过: ' + $diagnostics)
    }
    $destination = Join-Path $root ('target\qa-' + $case.Name + '.png')
    Copy-Item -LiteralPath $source -Destination $destination -Force
    if ($Archive -and !$case.Input) {
        Copy-Item -LiteralPath $source -Destination (Join-Path $root ('docs\screenshots\' + $case.Name + '.png')) -Force
    }
    Write-Output ('已生成: ' + $case.Name)
    $process.Dispose()
}
$configAfter = if (Test-Path -LiteralPath $configPath) { (Get-FileHash -LiteralPath $configPath).Hash } else { $null }
if ($configBefore -ne $configAfter) { throw '预览检查意外改变了用户配置' }
Write-Output '用户配置校验: 未改变'
