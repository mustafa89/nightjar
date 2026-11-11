import AppKit

/// Nightjar YouTube: YouTube Music's own web player in a window, reporting to Nightjar. A
/// background app (no Dock icon, no menu bar): Nightjar and the notch open its window, and it
/// quits when Nightjar quits.
///
/// Test aids: `--url <url>` opens another page first; `NJYT_SILENT=1` never autoplays and makes
/// no audio tap (`NJYT_TAP=1` adds a muted one); `NJYT_HIDDEN=1` never shows the window;
/// `NJYT_STATUS_PORT` / `NJYT_COMMAND_PORT` move it off the running app's ports.
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
        installShortcuts()
        quitWithNightjar()
        if ProcessInfo.processInfo.environment["NJYT_HIDDEN"] != "1" { player.show() }
    }

    /// A background app shows no menu bar, so its shortcuts are handled here: editing keys for
    /// the sign-in fields, and close, hide, reload and quit.
    private func installShortcuts() {
        NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            let mods = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
            guard mods.contains(.command), let key = event.charactersIgnoringModifiers?.lowercased() else { return event }
            let shift = mods.contains(.shift)
            let action: Selector? = switch key {
            case "x": #selector(NSText.cut(_:))
            case "c": #selector(NSText.copy(_:))
            case "v": #selector(NSText.paste(_:))
            case "a": #selector(NSText.selectAll(_:))
            case "z": shift ? Selector(("redo:")) : Selector(("undo:"))
            case "w": #selector(NSWindow.performClose(_:))
            case "h": #selector(NSApplication.hide(_:))
            case "q": #selector(NSApplication.terminate(_:))
            default: nil
            }
            if key == "r" {
                MainActor.assumeIsolated { self.player.reload() }
                return nil
            }
            guard let action else { return event }
            return NSApp.sendAction(action, to: nil, from: nil) ? nil : event
        }
    }

    /// The player belongs to Nightjar: when Nightjar quits, so does it.
    private func quitWithNightjar() {
        NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didTerminateApplicationNotification, object: nil, queue: .main
        ) { note in
            let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
            if app?.bundleIdentifier == "dev.mustafa.nightjar" { NSApp.terminate(nil) }
        }
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
    app.setActivationPolicy(.accessory)
    app.run()
}
