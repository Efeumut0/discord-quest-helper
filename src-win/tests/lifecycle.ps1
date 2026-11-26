$ErrorActionPreference = 'Stop'
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class RunnerTestWindow {
    public delegate bool EnumProc(IntPtr hwnd, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr w, IntPtr l);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, System.Text.StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    public static IntPtr Find(uint processId) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((hwnd, data) => {
            uint pid;
            GetWindowThreadProcessId(hwnd, out pid);
            var name = new System.Text.StringBuilder(256);
            GetClassName(hwnd, name, name.Capacity);
            if (pid == processId && name.ToString() == "DiscordQuestRunner") { result = hwnd; return false; }
            return true;
        }, IntPtr.Zero);
        return result;
    }
}
"@
$runnerRoot = Split-Path $PSScriptRoot -Parent
$runnerBuild = Join-Path $runnerRoot 'target/release/src-win.exe'
$runnerTestDir = Join-Path $runnerRoot ('target/lifecycle-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $runnerTestDir | Out-Null
foreach ($hidden in @($false, $true)) {
    $exe = Join-Path $runnerTestDir ('Named Game ' + $hidden + '.exe')
    Copy-Item -LiteralPath $runnerBuild -Destination $exe
    $runnerArgs = @('--title', 'RunnerLifecycleTest')
    if ($hidden) { $runnerArgs += '--tray' }
    $process = Start-Process -FilePath $exe -ArgumentList $runnerArgs -WindowStyle Hidden -PassThru
    $window = [IntPtr]::Zero
    try {
        $deadline = [DateTime]::UtcNow.AddSeconds(15)
        while ($window -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline -and !$process.HasExited) {
            Start-Sleep -Milliseconds 100
            $window = [RunnerTestWindow]::Find($process.Id)
        }
        if ($window -eq [IntPtr]::Zero) { throw 'Runner did not create its window.' }
        $process.Refresh()
        if ($process.Path -ne $exe) { throw "Unexpected executable path: $($process.Path)" }
        if (!$hidden -and ![RunnerTestWindow]::IsWindowVisible($window)) { throw 'Normal startup unexpectedly hid the window.' }
        # WM_CLOSE is the same close request sent by the title-bar X and Alt+F4.
        if (![RunnerTestWindow]::PostMessage($window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Close request failed.' }
        if (!$process.WaitForExit(5000)) { throw 'Runner survived WM_CLOSE.' }
        if ($process.ExitCode -ne 0) { throw "Unexpected exit code: $($process.ExitCode)" }
        Write-Output "PASS: named executable path and WM_CLOSE exit (tray requested: $hidden)"
    } finally {
        # Only the exact process started by this test is eligible for cleanup.
        if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
        $process.Dispose()
        Remove-Item -LiteralPath $exe
    }
}
Remove-Item -LiteralPath $runnerTestDir
