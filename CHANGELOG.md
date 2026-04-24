# Changelog

## v1.4.0

### Added
- **Log in with your Roblox account directly** — the Add Account dialog now has a "Log in with browser" option that opens a normal Roblox login window. Sign in as usual and RM will pick up your account automatically, with no need to copy cookies from your browser.

### Changed
- **Add Account dialog** — redesigned to ask how you'd like to add the account first (browser login or manual cookie paste), instead of showing both at once.
- **Cookie field** — when you do paste a cookie manually, the field is now a compact password-style input that hides the value, so the dialog stays small and your cookie isn't sitting on screen.
- **Master password prompt** — only appears when RM actually needs it. Once you've unlocked RM or set a master password, you won't be asked for it again when adding more accounts — and a mistyped password can no longer accidentally lock you out of the accounts you've already saved.

## v1.3.1

### Notice
- **Project moved to GitLab** — RM has moved from GitHub to GitLab. The new home is [gitlab.com/centerepic/robloxmanager](https://gitlab.com/centerepic/robloxmanager). Future releases and updates will be published there. The update checker has been switched to the new location.

## v1.3.0

### Added
- **Private server grouping** — private servers are now grouped by game with a thumbnail and game name in each group header.
- **Share link resolution** — paste an `rbxShareLink://` URL directly when adding a private server; RM resolves the access code automatically.
- **Game name and icon resolution** — game names and thumbnails are fetched in the background (no authentication required) and shown in the private servers tab.
- **Account groups** — accounts can be organised into named, colour-coded groups via drag-and-drop. Groups are collapsible and support bulk actions.
- **Custom account sorting** — accounts and groups can be reordered by dragging, or sorted alphabetically by name or by online status. Custom order is persisted across restarts.
- **Interactive first-launch tutorial** — new users see a 6-step guided walkthrough that highlights key UI elements (Add Account button, cookie field, account list, Launch button) and advances automatically as each action is completed.

### Fixed
- Private server name and icon were not resolving due to using an API endpoint that requires authentication. Switched to the unauthenticated `universeIds` endpoint.
- `universe_id` from the share link API response is now stored on the `PrivateServer` model and used for all subsequent name/icon lookups.
- UI no longer repaints continuously when idle; repaints are now triggered only when backend events arrive.

## v1.2.1

### Fixed
- **"What's New" window** — changelog now renders with proper formatting (headings, bold text, bullet points) instead of raw markdown.

## v1.2.0

### Added
- **Automatic update check** — on startup, checks GitLab for a newer release and shows a clickable "Update available" link in the top bar.
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
