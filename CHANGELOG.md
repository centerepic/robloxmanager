# Changelog

## v1.2.0

### Added
- **Automatic update check** — on startup, checks GitHub for a newer release and shows a clickable "Update available" link in the top bar.
- **"What's New" changelog** — on the first launch after an update, a window displays the changelog for the new version.
- **Standard data directory** — config and account data now stored in `%APPDATA%\RM` instead of next to the exe, so the app works from any location.
- **Legacy data migration** — if existing data is found next to the exe, a native dialog offers to move it to the new location on startup.
- **Version in title bar** — the window title now shows the current version number.

## v1.1.0

### Added
- **Anonymize names** — new toggle in Settings > Privacy that replaces all usernames and display names with generic "Account 1", "Account 2", etc. throughout the UI.

### Fixed
- **Favorite places** — clicking a favorite button now correctly populates the Place ID field. Previously an invisible overlapping widget was stealing clicks.
- **Favorite deletion** — right-clicking a favorite now shows a proper context menu with a "Remove" option, replacing the non-functional previous approach.
- Favorites row now wraps when there are many entries instead of overflowing off-screen.

## v1.0.0

- Initial release.
