# Windows helper

The generated executable runs as the process named in the game's catalog entry. Resource 101 stores its window title. Source 2 writes the Discord application ID to resource 102, which enables RPC over local Discord IPC. Source 1 and bulk creation leave resource 102 empty.

The window shows the RPC connection status. A sent status means Discord acknowledged the request. Failed connections retry with timeouts.

RPC activity doesn't confirm quest progress. Both Source 2 filename candidates use the same application ID.

Closing the window with X, Alt+F4 or Quit exits the process and closes its Discord connection. Minimizing to the tray keeps it running. `--tray` hides the window only if the tray icon was created.

`tests/lifecycle.ps1` checks process paths and window-close behavior. The optional live RPC test in `activity.rs` requires a running Discord desktop client and `DQH_RPC_SMOKE_APP_ID`.
