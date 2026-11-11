import AppKit

/// Nightjar YouTube: YouTube Music's own web player in a window, reporting to Nightjar.
/// `--url <url>` opens another page first; `NJYT_SILENT=1` analyses audio but plays nothing aloud
/// (for automated checks).
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    var player: Player!

    func applicationDidFinishLaunching(_ notification: Notification) {
        let args = CommandLine.arguments
        let start = args.firstIndex(of: "--url").flatMap { $0 + 1 < args.count ? URL(string: args[$0 + 1]) : nil } ?? Player.home
        let silent = ProcessInfo.processInfo.environment["NJYT_SILENT"] == "1"
        player = Player(start: start, silent: silent)
        if let snap = ProcessInfo.processInfo.environment["NJYT_SNAP"] { player.snapshotEvery5s(to: snap) }
        NSApp.mainMenu = menu()
        player.show()
    }

    /// The Dock icon or a second launch shows the window again.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        player.show()
        return true
    }

    @objc func reload() { player.reload() }
    @objc func goHome() { player.goHome() }

    /// App, Edit (paste into sign-in fields needs it), View and Window menus.
    private func menu() -> NSMenu {
        let bar = NSMenu()
        func submenu(_ title: String, _ items: [NSMenuItem]) {
            let holder = NSMenuItem()
            let m = NSMenu(title: title)
            items.forEach(m.addItem)
            holder.submenu = m
            bar.addItem(holder)
        }
        func item(_ title: String, _ action: Selector, _ key: String, _ mods: NSEvent.ModifierFlags = .command, target: AnyObject? = nil) -> NSMenuItem {
            let i = NSMenuItem(title: title, action: action, keyEquivalent: key)
            i.keyEquivalentModifierMask = mods
            i.target = target
            return i
        }
        submenu("Nightjar YouTube", [
            item("Hide", #selector(NSApplication.hide(_:)), "h"),
            .separator(),
            item("Quit", #selector(NSApplication.terminate(_:)), "q"),
        ])
        submenu("Edit", [
            item("Undo", Selector(("undo:")), "z"),
            item("Redo", Selector(("redo:")), "z", [.command, .shift]),
            .separator(),
            item("Cut", #selector(NSText.cut(_:)), "x"),
            item("Copy", #selector(NSText.copy(_:)), "c"),
            item("Paste", #selector(NSText.paste(_:)), "v"),
            item("Select All", #selector(NSText.selectAll(_:)), "a"),
        ])
        submenu("View", [
            item("Reload", #selector(reload), "r", target: self),
            item("YouTube Music Home", #selector(goHome), "H", [.command, .shift], target: self),
        ])
        submenu("Window", [
            item("Minimize", #selector(NSWindow.performMiniaturize(_:)), "m"),
            item("Close", #selector(NSWindow.performClose(_:)), "w"),
        ])
        return bar
    }
}

MainActor.assumeIsolated {
    let app = NSApplication.shared
    let delegate = AppDelegate()
    app.delegate = delegate
    app.setActivationPolicy(.regular)
    app.run()
}
