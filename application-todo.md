# Application Todo

## Critical fixes

- [ ] Investigate Tauri app instability when clicking buttons; identify the crash path, capture repro notes, and fix the underlying frontend/backend interaction.
- [x] Ensure startup bootstrap always creates `C:\BitOSDT\` before config, database, downloads, or workspace access on Windows.
- [x] Re-check the previously reported `run_dism` / `run_dism_with_role` compile issue in the current branch and remove stale notes if it no longer reproduces.

## Build/runtime features

- [x] Add separate Boot Drivers behavior for offline Windows image injection in `Full ISO` builds and a UNC boot-driver path option for `WDS/PXE` builds.
- [x] Keep Boot Drivers offline injection unselected by default.
- [x] Confirm boot-driver selection behavior for `Both` output mode and document that offline Windows injection maps only to the Full ISO half.

## WinPE

- [x] Issue when Booting winpe Taskbar class not registered. This needs to be thoroughly fixed.

## UX and workflow improvements

- [x] Make the wizard header and navigation remain visible while scrolling so the top controls and step actions stay accessible.
- [x] Automatically move the viewport to the build log area when `Start Build` is pressed.
- [x] Ensure the final build status/log area is brought into view when the build completes.
- [x] Prevent long Windows source names from overflowing summary cards or status boxes.
- [x] Ensure popups/modals stay inside the visible viewport without forcing a manual scroll hunt.
- [x] Re-validate the saved image profile loading fix; confirm `Modify Image` no longer sits on `Loading image profile...` and add better timeout/error UX if needed.

## Visual/theme work

- [x] Update the Space theme to move closer to `Logs and Issue/Artemis2.html`, including a stronger animated background treatment and higher-contrast glass surfaces.
- [x] Fix the "header/subnav" called BitOSDT Deployment Wizard Step 1 of 7 • Windows Source the location/of it seems to be broken as its not at the top etc. Investigate if other UI issues.

## Validation and investigation

- [ ] Verify whether the missing app icon in `npm run tauri dev` is dev-only or also affects packaged builds.
- [x] Confirm the fixed titlebar request required wizard-local sticky controls in addition to the already fixed global titlebar.
- [x] Change application version references from `2.0.5` to `2.0.6` for the app/package only, without running release workflow steps.
- [x] Review and remove stale dead-code or compile-warning notes that no longer correspond to current source locations.

## Completed

- [x] Investigate and fix why `Modify Image` can sit on `Loading image profile...` or take too long to load saved image profiles.

## Archived / stale logs

- Historical compiler notes about a missing `run_dism` import were recorded here previously; current source already contains both `run_dism` and `run_dism_with_role`, so the old log has been archived until a fresh repro exists.
- Historical dead-code warning dumps from older `src\main.rs` paths were condensed into tracked validation items instead of staying inline as raw log spam.
