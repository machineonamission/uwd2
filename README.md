# ![UWD2](assets/banner.png)

# Universal Watermark Disabler 2

![demo](assets/demo.png)

Created by [Melody](https://machineonamission.me/)

Inspired by [Universal Watermark Disabler](https://github.com/pr701/universal-watermark-disabler)
by [Painter701](https://github.com/pr701)

Written in [Rust](https://www.rust-lang.org/)

## What is UWD2?

UWD2 removes that pesky watermark in the corner of Windows Insider builds, as well as other similar types of watermarks.

## How to use it

Run UWD2 to open its Windows interface. Click **Remove watermark now** to apply the change to the current Explorer session. Use **Enable at startup** to have UWD2 apply the change when you sign in, or **Disable at startup** to remove it from the current user's Startup folder. The startup setting affects future sign-ins only.

Use **Use dark mode** or **Use light mode** to switch the interface theme. UWD2 saves the theme choice for the next launch.

The command-line interface remains available for the current Explorer session: `uwd2.exe enable`, `uwd2.exe disable`, `uwd2.exe status`, and `uwd2.exe help`.

## Some disclaimers

### **UWD2 DOES NOT REMOVE THE "ACTIVATE WINDOWS" WATERMARK!!!**

UWD2 is for the insider beta watermark.

**UWD2 DOES NOT persist between explorer.exe or system restarts**. [See why below](#how-does-it-work). Enable startup in the interface to reapply it at sign-in.

UWD2 requires an internet connection on first run and between some system updates. [See why below](#how-does-it-work).

UWD2 uses architecture-specific return instructions for x86-64 and ARM64. ARM64 has not been verified on a Windows Insider build.

UWD2 has only been tested on Windows insider beta watermarks. It may work on other similar watermark such as "test
mode", but these are untested.

## How does it work?

UWD2 takes an entirely different approach than the original UWD.
Using [WinDbg](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/), I found that inside shell32.dll
there is a function called `CDesktopWatermark::s_DesktopBuildPaint`. This function is what paints the watermark on the
desktop. Using this knowledge, UWD2:

- downloads debugging symbols from microsoft (this is why UWD2 needs an internet connection. UWD2 also caches these
  locally)
- Uses those symbols to find the memory location of `CDesktopWatermark::s_DesktopBuildPaint`
- Inserts an architecture-specific `ret` instruction into the memory of the running
  explorer.exe (this is why UWD2 does not persist) at the position of the `CDesktopWatermark::s_DesktopBuildPaint`
  function, causing the function's code to never execute.
