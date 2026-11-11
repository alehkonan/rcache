//! Knowledge base: what each cache is, whether it is safe to delete, and how to clean it.
//!
//! Rules are matched against the displayed path (`~/…` for the home directory),
//! ASCII case-insensitively, first match wins. In patterns `*` matches within one
//! path component and `**` matches across components. The last rules are
//! catch-alls, so every location gets an explanation.

/// How safe it is to delete a cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Safety {
    /// Pure cache: rebuilt or re-downloaded automatically, nothing of yours is lost.
    Safe,
    /// Regenerable, but with a cost or a catch (big re-downloads, app must be quit,
    /// system-managed, or it also holds some data you may care about).
    Caution,
    /// Holds data that is not reproducible. Clean it selectively, not with `rm -rf`.
    Keep,
}

impl Safety {
    pub fn label(self) -> &'static str {
        match self {
            Safety::Safe => "safe",
            Safety::Caution => "caution",
            Safety::Keep => "keep",
        }
    }
}

/// How to reclaim the space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clean {
    /// Delete the whole location; whoever owns it recreates it.
    Remove,
    /// Delete what is inside the location but keep the folder itself.
    RemoveContents,
    /// A tool-specific command or instruction, shown verbatim.
    Command(&'static str),
}

/// Explanation of one cache location.
#[derive(Debug)]
pub struct Info {
    /// What the cache holds and who uses it.
    pub what: &'static str,
    pub safety: Safety,
    /// Why it has that safety rating / what happens after deleting it.
    pub why: &'static str,
    pub clean: Clean,
}

struct Rule {
    pattern: &'static str,
    info: Info,
}

const fn rule(pattern: &'static str, safety: Safety, what: &'static str, why: &'static str, clean: Clean) -> Rule {
    Rule { pattern, info: Info { what, safety, why, clean } }
}

use Clean::{Command as Cmd, Remove as RM, RemoveContents as RM_CONTENTS};
use Safety::{Caution, Keep, Safe};

const QUIT_APP: &str = "The app rebuilds it; quit the app first so it doesn't hold files open";
const REDOWNLOAD: &str = "Re-downloaded on demand; only costs bandwidth and time on the next use";

#[rustfmt::skip]
static RULES: &[Rule] = &[
    // ── Xcode / Apple development ──────────────────────────────────────────
    rule("~/Library/Developer/Xcode/DerivedData", Safe,
        "Xcode build products, intermediates and code indexes for every project you've opened",
        "Rebuilt on the next build; the first build and indexing will be slower. Quit Xcode first",
        RM_CONTENTS),
    rule("~/Library/Developer/Xcode/Archives", Keep,
        "Archived app builds (.xcarchive) with the dSYMs needed to symbolicate crash reports",
        "Cannot be regenerated for builds already shipped; delete old ones selectively",
        Cmd("Xcode ▸ Window ▸ Organizer ▸ Archives, delete the versions you no longer support")),
    rule("~/Library/Developer/Xcode/* DeviceSupport", Safe,
        "Debug symbols copied from each OS version of devices you've connected to Xcode",
        "Copied again (takes a few minutes) the next time a device with that OS version connects",
        RM_CONTENTS),
    rule("~/Library/Developer/CoreSimulator", Caution,
        "iOS/watchOS/tvOS simulator devices, including apps installed on them and their data",
        "Deleting by hand corrupts simctl's state; erase or delete devices through Xcode tools",
        Cmd("xcrun simctl delete unavailable   (and `xcrun simctl erase all` to reset the rest)")),
    rule("~/.cocoapods", Safe,
        "CocoaPods spec repositories (the index of available pods)",
        "Fetched again on the next `pod install`",
        RM),
    rule("**/Library/Caches/CocoaPods", Safe,
        "CocoaPods download cache of pod sources",
        REDOWNLOAD,
        Cmd("pod cache clean --all")),
    rule("**/Library/Caches/com.apple.dt.*", Safe,
        "Xcode and developer-tools caches",
        "Rebuilt by Xcode; quit Xcode first",
        RM),

    // ── JavaScript / Node.js ───────────────────────────────────────────────
    rule("~/.npm/_cacache", Safe,
        "npm's content-addressable cache of downloaded package tarballs and metadata",
        REDOWNLOAD,
        Cmd("npm cache clean --force")),
    rule("~/.npm/_npx", Safe,
        "Packages downloaded to run `npx <tool>` without installing them",
        "Downloaded again the next time you run the same npx command",
        RM),
    rule("~/.npm/_logs", Safe,
        "npm debug logs from past commands",
        "Only useful for troubleshooting a failed npm run",
        RM),
    rule("**/Library/Caches/Yarn", Safe,
        "Yarn 1 (classic) global cache of downloaded packages",
        "Re-downloaded on the next install; offline installs stop working until then",
        Cmd("yarn cache clean")),
    rule("~/.yarn/berry/cache", Safe,
        "Yarn 2+ (Berry) global package mirror",
        REDOWNLOAD,
        Cmd("yarn cache clean --all")),
    rule("~/Library/pnpm/store", Caution,
        "pnpm content-addressable store; projects' node_modules hard-link their files from here",
        "Files still used by a project are shared with its node_modules, so deleting the store frees less than shown and forces reinstalls. Prune unreferenced packages instead",
        Cmd("pnpm store prune")),
    rule("**/Library/Caches/pnpm", Safe,
        "pnpm package-metadata cache and `pnpm dlx` downloads",
        REDOWNLOAD,
        RM),
    rule("~/.bun/install/cache", Safe,
        "Bun's global package cache",
        REDOWNLOAD,
        Cmd("bun pm cache rm")),
    rule("**/Library/Caches/bun", Safe,
        "Bun runtime cache (transpiled files, downloads)",
        "Rebuilt automatically",
        RM),
    rule("**/Library/Caches/deno", Safe,
        "Deno's cache of remote modules, npm packages and compiled code",
        REDOWNLOAD,
        Cmd("deno clean")),
    rule("~/.cache/node", Safe,
        "Corepack's downloaded package-manager binaries (pnpm, yarn) under node/corepack",
        "Corepack downloads the pinned version again when a project needs it",
        Cmd("corepack cache clean")),
    rule("**/Library/Caches/node-gyp", Safe,
        "Node.js headers downloaded to compile native add-ons",
        "Downloaded again the next time a native module is built",
        RM),
    rule("**/Library/Caches/typescript", Safe,
        "TypeScript's automatic type acquisition: @types packages fetched by editors for JS projects",
        "The editor's TS server fetches them again when needed",
        RM),
    rule("**/Library/Caches/Cypress", Safe,
        "Cypress test-runner app binaries, one per installed Cypress version",
        "The binary is reinstalled by `npx cypress install`; prune keeps only the current version",
        Cmd("npx cypress cache prune")),
    rule("**/Library/Caches/ms-playwright", Safe,
        "Browser builds (Chromium, Firefox, WebKit) downloaded by Playwright",
        "Tests need `npx playwright install` before the next run",
        Cmd("npx playwright uninstall --all")),
    rule("**/Library/Caches/ms-playwright-mcp", Caution,
        "Browser profile used by the Playwright MCP server (cache, cookies, logins)",
        "Rebuilt automatically, but sites logged in through the MCP browser are logged out",
        RM),
    rule("**/Library/Caches/org.webkit.Playwright", Safe,
        "Web cache of Playwright's WebKit browser",
        "Rebuilt automatically",
        RM),
    rule("~/.cache/puppeteer", Safe,
        "Chrome / chrome-headless-shell builds downloaded by Puppeteer",
        "Downloaded again on the next `npm install` of puppeteer or `npx puppeteer browsers install`",
        RM),
    rule("~/.cache/prisma", Safe,
        "Prisma query/schema engine binaries",
        "Downloaded again by `prisma generate`",
        RM),
    rule("**/Library/Caches/prisma-nodejs", Safe,
        "Prisma CLI cache",
        "Rebuilt automatically",
        RM),
    rule("**/Library/Caches/checkpoint-nodejs", Safe,
        "Prisma's update-check state",
        "Rebuilt automatically",
        RM),
    rule("**/Library/Caches/com.amazonaws.jsii", Safe,
        "jsii runtime cache used by AWS CDK to run constructs in other languages",
        "Rebuilt on the next `cdk` command",
        RM),

    // ── Python ─────────────────────────────────────────────────────────────
    rule("**/Library/Caches/pip", Safe,
        "pip's cache of downloaded and built wheels",
        REDOWNLOAD,
        Cmd("python3 -m pip cache purge")),
    rule("**/Library/Caches/pypoetry", Caution,
        "Poetry's package cache and, by default, the virtualenvs of your Poetry projects",
        "The package cache is safe, but deleting the virtualenvs folder means re-running `poetry install` in each project",
        Cmd("poetry cache clear PyPI --all")),
    rule("~/.cache/uv", Safe,
        "uv's cache of downloaded packages, built wheels and Python installs",
        REDOWNLOAD,
        Cmd("uv cache clean")),
    rule("~/.cache/pre-commit", Safe,
        "Environments for pre-commit hooks",
        "Rebuilt the next time hooks run",
        Cmd("pre-commit clean")),
    rule("~/.cache/huggingface", Caution,
        "Hugging Face models and datasets",
        "Safe to delete, but models can be many GB to download again",
        Cmd("huggingface-cli delete-cache   (pick what to delete)")),
    rule("**/Library/Caches/pylint", Safe,
        "pylint analysis results",
        "Rebuilt on the next run",
        RM),
    rule("**/Library/Caches/com.apple.python", Safe,
        "Bytecode (.pyc) cache for macOS's bundled Python",
        "Recompiled automatically",
        RM),

    // ── Go, Rust, JVM, PHP ─────────────────────────────────────────────────
    rule("**/Library/Caches/go-build", Safe,
        "Go build cache (compiled packages and test results)",
        "Rebuilt automatically; the next builds are slower",
        Cmd("go clean -cache")),
    rule("~/go/pkg/mod", Safe,
        "Go module download cache",
        "Re-downloaded on the next build; offline builds fail until then",
        Cmd("go clean -modcache")),
    rule("**/Library/Caches/gopls", Safe,
        "gopls (Go language server) index",
        "Rebuilt when the editor opens a Go project",
        RM),
    rule("**/Library/Caches/goimports", Safe,
        "goimports package index",
        "Rebuilt automatically",
        RM),
    rule("**/Library/Caches/staticcheck", Safe,
        "staticcheck analysis cache",
        "Rebuilt on the next run",
        RM),
    rule("**/Library/Caches/go", Safe,
        "Go tooling cache (e.g. telemetry, vulnerability DB)",
        "Rebuilt automatically",
        RM),
    rule("~/.cargo/registry", Safe,
        "Downloaded crate archives (cache/) and their unpacked sources (src/), plus the registry index",
        "Cargo re-downloads crates on the next build; offline builds fail until then",
        Cmd("rm -rf ~/.cargo/registry/cache ~/.cargo/registry/src")),
    rule("~/.cargo/git", Safe,
        "Checkouts of git dependencies",
        REDOWNLOAD,
        RM),
    rule("~/.gradle/caches", Safe,
        "Gradle dependency cache, transforms and build cache",
        "Re-downloaded and rebuilt on the next build; stop daemons first",
        Cmd("gradle --stop; rm -rf ~/.gradle/caches")),
    rule("~/.gradle/wrapper/dists", Safe,
        "Gradle distributions downloaded by projects' gradlew wrappers",
        "Each project's wrapper downloads its version again",
        RM_CONTENTS),
    rule("~/.m2/repository", Caution,
        "Maven local repository: downloaded dependencies and artifacts you `mvn install`ed",
        "Downloads come back automatically, but locally-installed, unpublished artifacts are lost",
        RM),
    rule("~/.composer/cache", Safe,
        "Composer (PHP) package cache",
        REDOWNLOAD,
        Cmd("composer clear-cache")),
    rule("**/Library/Caches/composer", Safe,
        "Composer (PHP) package cache",
        REDOWNLOAD,
        Cmd("composer clear-cache")),

    // ── Containers, cloud and security tools ───────────────────────────────
    rule("**/Library/Caches/colima", Safe,
        "Colima's downloaded VM disk images (not your VMs; those live in ~/.colima)",
        "Downloaded again only when a new VM is created",
        RM),
    rule("**/Library/Caches/lima", Safe,
        "Lima's downloaded VM images (not your VMs; those live in ~/.lima)",
        "Downloaded again only when a new VM is created",
        RM),
    rule("**/Library/Caches/Docker Desktop", Safe,
        "Docker Desktop app cache. Images and containers are NOT here; they live in the Docker VM",
        "Rebuilt automatically; quit Docker Desktop first. To free image space use `docker system prune`",
        RM),
    rule("**/Library/Caches/trivy", Safe,
        "Trivy vulnerability database and scan cache",
        "The DB is re-downloaded on the next scan",
        Cmd("trivy clean --all")),
    rule("**/Library/Caches/fanal", Safe,
        "Cache of older Trivy versions (image layer analysis)",
        "Rebuilt on the next scan",
        RM),
    rule("**/Library/Caches/aws", Safe,
        "AWS IDE toolkits' downloads (e.g. the Amazon Q / CodeWhisperer language server)",
        "Downloaded again by the IDE extension",
        RM),

    // ── Editors and AI tools ───────────────────────────────────────────────
    rule("**/Library/Caches/JetBrains", Safe,
        "JetBrains IDE caches and indexes, one folder per IDE version",
        "Folders of IDE versions you no longer use are dead weight; current ones re-index on next start",
        Cmd("delete old-version folders, or in the IDE: File ▸ Invalidate Caches…")),
    rule("~/Library/Application Support/Code/*", Safe,
        "VS Code HTTP / compiled-code / extension-download caches",
        "Rebuilt automatically; quit VS Code first",
        RM),
    rule("**/Library/Caches/Code", Safe,
        "VS Code cache",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/com.microsoft.VSCode", Safe,
        "VS Code cache",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/Zed", Safe,
        "Zed editor cache",
        QUIT_APP,
        RM),
    rule("~/.cache/zed", Safe,
        "Zed editor cache",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/com.todesktop.230313mzl4w4u92", Safe,
        "Cursor editor cache",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/claude-cli-nodejs", Safe,
        "Claude Code CLI per-project cache (mainly MCP server logs)",
        "Only logs; recreated automatically",
        RM),
    rule("**/Library/Caches/com.anthropic.claudefordesktop", Safe,
        "Claude desktop app cache",
        QUIT_APP,
        RM),

    // ── Browsers ───────────────────────────────────────────────────────────
    rule("**/Library/Caches/Google", Safe,
        "Google Chrome page/media cache (history, passwords and logins are elsewhere)",
        "Sites load slower until it refills; quit Chrome first",
        RM),
    rule("**/Library/Caches/Arc", Safe,
        "Arc browser page/media cache (history and logins are elsewhere)",
        "Sites load slower until it refills; quit Arc first",
        RM),
    rule("**/Library/Caches/company.thebrowser.*", Safe,
        "Arc browser cache",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/Firefox", Safe,
        "Firefox page/media cache (history, passwords and logins are elsewhere)",
        "Sites load slower until it refills; quit Firefox first",
        RM),
    rule("**/Library/Caches/Mozilla", Safe,
        "Downloaded Firefox updates",
        "Downloaded again if an update is still pending",
        RM),
    rule("**/Library/Caches/com.microsoft.edgemac*", Safe,
        "Microsoft Edge page/media cache",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/BraveSoftware", Safe,
        "Brave browser page/media cache",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/com.apple.Safari", Safe,
        "Safari page/media cache",
        "Rebuilt automatically; macOS protects this folder, so use Safari's own command",
        Cmd("Safari ▸ Develop ▸ Empty Caches (enable the Develop menu in Settings ▸ Advanced)")),
    rule("**/Library/Caches/com.apple.Safari.SafeBrowsing", Safe,
        "Safari's Safe Browsing (phishing/malware) database",
        "Re-downloaded automatically",
        RM),

    // ── Apps ───────────────────────────────────────────────────────────────
    rule("**/Library/Caches/com.spotify.client", Caution,
        "Spotify streaming cache, and by default also the songs you downloaded for offline listening",
        "Offline downloads have to be downloaded again; clear it from inside Spotify",
        Cmd("Spotify ▸ Settings ▸ Storage ▸ Clear cache")),
    rule("**/Library/Caches/com.tinyspeck.slackmacgap", Safe,
        "Slack cache",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/com.microsoft.teams*", Safe,
        "Microsoft Teams cache",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/com.figma.*", Safe,
        "Figma desktop cache",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/com.postmanlabs.mac", Safe,
        "Postman cache (collections are synced to your account or stored elsewhere)",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/notion.id", Safe,
        "Notion desktop cache",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/com.mongodb.compass", Safe,
        "MongoDB Compass cache",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/com.usebruno.app", Safe,
        "Bruno API client cache (collections are files in your projects)",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/*OneDrive*", Safe,
        "OneDrive app cache (your files live in ~/Library/CloudStorage, not here)",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/com.microsoft.SharePoint-mac", Safe,
        "OneDrive/SharePoint sync helper cache",
        QUIT_APP,
        RM),
    rule("**/Library/Caches/Adobe", Safe,
        "Adobe apps' shared cache",
        "Rebuilt automatically; quit Adobe apps first",
        RM),
    rule("**/Library/Caches/com.adobe.*", Safe,
        "Adobe app / helper cache",
        "Rebuilt automatically; quit Adobe apps first",
        RM),
    rule("**/Library/Caches/Acrobat", Safe,
        "Adobe Acrobat cache",
        QUIT_APP,
        RM),

    // ── Auto-updaters, crash reporters, telemetry ──────────────────────────
    rule("**/Library/Caches/*.ShipIt", Safe,
        "Squirrel auto-updater staging folder: downloaded update packages of an Electron app",
        "Downloaded again if an update is still pending; quit the app first",
        RM),
    rule("**/Library/Caches/*updater*", Safe,
        "Auto-updater downloads",
        "Downloaded again if an update is still pending",
        RM),
    rule("**/Library/Caches/com.microsoft.autoupdate*", Safe,
        "Microsoft AutoUpdate downloads",
        "Downloaded again if an update is still pending",
        RM),
    rule("**/Library/Caches/com.google.Keystone", Safe,
        "Google Software Update (Keystone) downloads",
        "Downloaded again if an update is still pending",
        RM),
    rule("**/Library/Caches/com.google.SoftwareUpdate", Safe,
        "Google Software Update downloads",
        "Downloaded again if an update is still pending",
        RM),
    rule("**/Library/Caches/*crash*", Safe,
        "Crash-reporter queue of an app (reports not yet sent)",
        "Only unsent crash reports are lost",
        RM),
    rule("**/Library/Caches/io.sentry", Safe,
        "Sentry crash/error-reporting queue",
        "Only unsent reports are lost",
        RM),
    rule("**/Library/Caches/com.otel.*", Safe,
        "OpenTelemetry buffer (telemetry not yet sent)",
        "Only unsent telemetry is lost",
        RM),
    rule("**/Library/Caches/com.datadoghq", Safe,
        "Datadog SDK telemetry buffer",
        "Only unsent telemetry is lost",
        RM),

    // ── Homebrew ───────────────────────────────────────────────────────────
    rule("**/Library/Caches/Homebrew", Safe,
        "Homebrew downloads: bottles and source archives kept after installing",
        "Installed packages are unaffected; downloads come back only if you reinstall",
        Cmd("brew cleanup --prune=all")),

    // ── macOS ──────────────────────────────────────────────────────────────
    rule("~/Library/Logs", Safe,
        "Logs written by apps, plus crash reports (DiagnosticReports)",
        "Nothing depends on them, but they help when troubleshooting a recent problem",
        RM_CONTENTS),
    rule("**/Library/Caches/GeoServices", Safe,
        "Apple Maps tiles and geocoding cache",
        "Re-downloaded as you use Maps",
        RM),
    rule("**/Library/Caches/CloudKit", Caution,
        "iCloud (CloudKit) sync cache shared by apps",
        "Managed by macOS; deleting can force a full re-sync. macOS trims it itself under disk pressure",
        Cmd("let macOS manage it")),
    rule("/Library/Caches/com.apple.iconservices.store", Caution,
        "System-wide app icon cache",
        "Rebuilt only after a restart; icons may look blank until then",
        Cmd("sudo rm -rf /Library/Caches/com.apple.iconservices.store && sudo killall Dock Finder")),
    rule("**/Library/Caches/com.apple.*", Caution,
        "Cache of a macOS system service or Apple app",
        "Managed by macOS, which purges it automatically when disk space runs low; many are protected and the service may need a restart",
        RM),

    // ── Catch-alls ─────────────────────────────────────────────────────────
    rule("~/.npm/*", Caution,
        "npm data folder",
        "Not a known npm cache folder; check what it is before deleting",
        RM),
    rule("~/.cache/*", Safe,
        "XDG cache folder of a command-line tool",
        "By convention ~/.cache holds only regenerable data",
        RM),
    rule("~/Library/Caches/*", Safe,
        "Cache folder of the app or tool named in the path",
        "macOS apps must be able to rebuild anything in Caches; quit the app first",
        RM),
    rule("/Library/Caches/*", Caution,
        "System-wide cache shared by all users (named after the app or service)",
        "Usually regenerable but owned by root; needs sudo and the owning service may need a restart",
        RM),
    rule("**", Caution,
        "Unrecognised location",
        "Inspect it before deleting",
        RM),
];

/// Explanation for a location, given its displayed path (`~/…` for home).
pub fn explain(display_path: &str) -> &'static Info {
    let rule = RULES
        .iter()
        .find(|r| glob(r.pattern.as_bytes(), display_path.as_bytes()))
        .expect("RULES ends with a catch-all");
    &rule.info
}

/// `*` matches within a path component, `**` across components; ASCII case-insensitive.
fn glob(pattern: &[u8], text: &[u8]) -> bool {
    match pattern {
        [] => text.is_empty(),
        [b'*', b'*', rest @ ..] => (0..=text.len()).any(|i| glob(rest, &text[i..])),
        [b'*', rest @ ..] => {
            for i in 0..=text.len() {
                if glob(rest, &text[i..]) {
                    return true;
                }
                if text.get(i) == Some(&b'/') {
                    return false;
                }
            }
            false
        }
        [p, rest @ ..] => text
            .split_first()
            .is_some_and(|(t, text)| t.eq_ignore_ascii_case(p) && glob(rest, text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locations::DEDICATED;

    #[test]
    fn glob_semantics() {
        assert!(glob(b"~/Library/Caches/*", b"~/Library/Caches/Foo"));
        assert!(!glob(b"~/Library/Caches/*", b"~/Library/Caches/Foo/Bar"));
        assert!(glob(b"**/Library/Caches/x", b"~/Library/Caches/X"));
        assert!(glob(b"**/Library/Caches/x", b"/Library/Caches/x"));
        assert!(glob(b"**/Library/Caches/*.ShipIt", b"~/Library/Caches/com.figma.Desktop.ShipIt"));
        assert!(!glob(b"~/x", b"~/xy"));
    }

    #[test]
    fn specific_rules_win_over_catch_alls() {
        assert_eq!(explain("~/Library/Caches/Homebrew").clean, Clean::Command("brew cleanup --prune=all"));
        assert_eq!(explain("~/Library/Caches/com.apple.Safari").safety, Safety::Safe);
        assert_eq!(explain("~/Library/Caches/com.apple.akd").safety, Safety::Caution);
        assert_eq!(explain("~/Library/Caches/com.usebruno.app.ShipIt").what, explain("~/Library/Caches/x.ShipIt").what);
        assert_eq!(explain("/Library/Caches/Unknown").safety, Safety::Caution);
        assert_eq!(explain("~/Library/Caches/SomeApp").safety, Safety::Safe);
    }

    #[test]
    fn every_dedicated_location_has_its_own_rule() {
        let catch_alls = ["~/.npm/*", "~/.cache/*", "~/Library/Caches/*", "/Library/Caches/*", "**"];
        for path in DEDICATED {
            let matched = RULES.iter().find(|r| glob(r.pattern.as_bytes(), path.as_bytes())).unwrap();
            assert!(!catch_alls.contains(&matched.pattern), "{path} only matches a catch-all");
        }
    }
}
