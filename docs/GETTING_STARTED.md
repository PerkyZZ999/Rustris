# Getting started with Rustris

Rustris is a local Linux launcher under development. It manages references to
existing games; it does not install games, runners or dependencies. The current
primary-host build and its known limits are summarised in the [README](../README.md#project-status).

## Build and open

Install the prerequisites listed in the [README](../README.md), then from the
project directory run:

```sh
cargo build --release --workspace --locked
./target/release/rustris
```

Keep `rustris-session` beside `rustris`: the desktop uses that helper for launches
and session control. The first open creates an owned library under the XDG data
directory; it does not import another launcher's library.

## Finding your way around

The sidebar opens the **Library**, **Favorites**, **Sessions**, **Runners** and
**Settings**. Selecting a game opens its page: **Play**, **Configure** and the
favourite heart sit beside the title, and the tabs below show the **Overview**
(recent sessions, artwork, removal), **Configuration**, **Files** and **Logs**.
**Library** in the sidebar or the breadcrumb returns to the grid; `Escape` does
the same from a game's page. `Ctrl+B` collapses the sidebar. The title bar holds
the search field, the sessions indicator, Settings and the window controls; drag
the title bar to move the window and double-click it to maximise.

## Add a native game

1. Choose **Add game** and enter a name and the absolute path to an existing Linux
   executable. Leave the working directory blank to use the executable's parent,
   or enter a different absolute directory.
2. Add each argument as a separate row. Spaces in one row stay in that argument;
   Rustris does not interpret it as a shell command. Environment rows can set a
   value, set an empty value or explicitly unset a key.
3. Save the profile (`Ctrl+S` works too); the game's page opens. Choose **Play**;
   the **Logs** tab shows the session, and **Preflight evidence** there lists the
   availability observations. A successful check or a running
   process does not establish that the game is playable.

Rustris can save a path that is currently unavailable; reconnecting a drive does
not require recreating the entry. Removing an entry removes Rustris's own record,
not the game installation.

## Add an existing UMU/Proton game

1. In **Runners** (or **Settings → Runner settings**), choose one of the
   installed Proton builds Rustris found; it fills in the UMU launcher, the Proton
   folder and the runtime that build needs. **Advanced settings** shows the three
   paths for manual entry, each with a **Browse…** button. One selection applies to
   UMU launches until you change it. Rustris does not install or update these tools.
2. Add a game or choose **Configure** on its page, pick the **UMU / Proton** card
   under Runner, and enter the existing Windows
   executable, working directory and flat Wine-prefix directory. A flat prefix
   can contain `pfx` pointing back to itself; a nested compatibility-data parent
   is not accepted as the prefix.
3. Choose **Review and play**. Check the displayed paths, versions, arguments,
   environment and findings before choosing **Launch game**. Cancel leaves the
   review without starting a session. After you launch, the button becomes
   **Play** and starts the game directly while nothing in the review changes;
   editing the profile or choosing other runners shows the review again.

The validated combinations are UMU 1.4.3 with either UMU-Proton-10.0-4 and
Sniper `3.0.20260805.254768`, or GE-Proton11-5 build `1786437966` and steamrt4
`4.0.20261021.265997`. Other detected builds can be used and are labelled **Not
validated** in Runners and in the review. For an existing populated prefix, a missing or different Proton
version marker blocks launch. A matching marker is not a health check. Do not
switch or repair a prefix simply to clear a warning; inspect its state and backups
first. Normal game and prefix writes can occur during launch.

Runner output is captured, but Windows application console output is unverified.
A missing message or a zero exit is not evidence of game health. The real-title
checks so far are bounded startup/Stop observations, not gameplay guarantees.

## Artwork and keyboard navigation

Use **Choose artwork…** or **Enter artwork path…** in the Artwork panel of a game's
Overview. Games without artwork show a generated cover. To have Rustris find
artwork for you, turn on **Download artwork automatically** in Settings; new games
then get a cover and banner, **Download missing artwork** fills in the rest of the
library, and a game's Artwork panel offers **Download artwork**. The Steam Store
needs no account; for games not sold on Steam, save a free SteamGridDB API key in
the same Settings section. Game names are sent to these services only while the
setting is on.
Rustris stores a reference; clearing it does not delete the image. Missing or
unreadable images display a placeholder. Exact Unix byte mode can retain a path
that Qt cannot display; that reference also uses a placeholder.

`Ctrl+N` opens a new profile and `Ctrl+F` or `Ctrl+K` focuses search; Down moves
from search into the grid, the arrow keys move through it, and Enter opens a game. In the tested Qt fallback
file picker, `Ctrl+L` followed by an absolute path and Enter selects a file. Other
platform picker implementations may behave differently. The separate path form is
also keyboard accessible in the tested private desktop.

## Sessions and owned data

A game's **Logs** tab, and the **Sessions** page for every session, show state,
captured output and preflight evidence. **Stop**
acts on the currently verified owned process scope; **Force stop** requires a prior
Stop and confirmation. Closing the desktop can leave helpers observing sessions.
If tracking is unknown, the reservation stays until reconciliation or an explicit
warned release; releasing it does not stop a game.

The library defaults to `~/.local/share/rustris/`, configuration to
`~/.config/rustris/`, and logs to `~/.local/state/rustris/`; XDG overrides apply.
Quit before copying the complete owned data directory, including SQLite WAL/SHM
files if present. A verified source-schema backup is created before an upgrade;
`.incomplete` files are not verified backups. Preserve data on errors rather than
overwriting a live database. Automatic restore and power-loss recovery are not
established.
