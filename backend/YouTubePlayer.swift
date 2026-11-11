import AppKit
import WebKit

/// The YouTube Music window inside Nightjar: music.youtube.com in a WKWebView with the Nightjar
/// page script, sampled 30 times a second while playing (twice a second otherwise) with the
/// spectrum from the audio tap. Each status goes to `onStatus` (the bridge to Rust).
/// Closing the window hides it; the music keeps playing.
@MainActor
final class YouTubePlayer: NSObject, NSWindowDelegate, WKUIDelegate {
    nonisolated static let home = URL(string: "https://music.youtube.com/")!
    /// Google refuses sign-in from browsers it doesn't recognise; present as Safari.
    static let userAgent =
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Safari/605.1.15"

    let window: NSWindow
    let webView: WKWebView
    var onStatus: ((String) -> Void)?
    private let tap: AudioTap
    private let tapEnabled: Bool
    /// Tap set-up can wait on the permission prompt; never on the main thread.
    private let tapQueue = DispatchQueue(label: "nightjar.youtube.tap")
    private var timer: Timer?
    private var keys: Any?
    private var tick = 0
    private var lastState = ""

    /// `silent` (tests): a throwaway web session, no autoplay, every media element muted, and
    /// no audio tap unless `tap` asks for a muted one.
    init(start: URL, script: String, silent: Bool, tap tapInSilent: Bool) {
        let config = WKWebViewConfiguration()
        // Persistent, so you sign in once; test runs get a throwaway store and never touch it.
        config.websiteDataStore = silent ? .nonPersistent() : .default()
        config.mediaTypesRequiringUserActionForPlayback = silent ? .all : []
        let scripts = config.userContentController
        if silent {
            let mute = "for (const e of ['loadstart', 'play', 'playing']) document.addEventListener(e, (ev) => { if (ev.target instanceof HTMLMediaElement) { ev.target.muted = true; ev.target.volume = 0; } }, true);"
            scripts.addUserScript(WKUserScript(source: mute, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        }
        scripts.addUserScript(WKUserScript(source: script, injectionTime: .atDocumentStart, forMainFrameOnly: true))

        webView = WKWebView(frame: .zero, configuration: config)
        webView.customUserAgent = Self.userAgent
        webView.allowsBackForwardNavigationGestures = true

        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1180, height: 780),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "YouTube Music · Nightjar"
        window.setFrameAutosaveName("NightjarYouTube")
        window.contentView = webView
        window.isReleasedWhenClosed = false
        if !window.setFrameUsingName("NightjarYouTube") { window.center() }

        tap = AudioTap(muted: silent)
        tapEnabled = !silent || tapInSilent
        super.init()
        window.delegate = self
        webView.uiDelegate = self
        installShortcuts()

        if silent {
            // A throwaway store has no consent choice yet: "reject all" (SOCS=CAI) skips the EU
            // consent page, as answering it once would.
            let consent = HTTPCookie(properties: [.domain: ".youtube.com", .path: "/", .name: "SOCS", .value: "CAI", .secure: true])!
            let view = webView
            config.websiteDataStore.httpCookieStore.setCookie(consent) { view.load(URLRequest(url: start)) }
        } else {
            webView.load(URLRequest(url: start))
        }

        let timer = Timer(timeInterval: 1.0 / 30.0, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.sample() }
        }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    func show() {
        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
    }

    /// Every frame while playing, every 15th (twice a second) otherwise.
    private func sample() {
        tick += 1
        // Once a second: follow WebKit's audio processes as they start and restart.
        if tapEnabled, tick % 30 == 0 { tapQueue.async { [tap] in tap.refresh() } }
        if lastState != "playing", tick % 15 != 0 { return }
        webView.evaluateJavaScript("window.__nj ? window.__nj.sample() : ''") { [weak self] result, _ in
            MainActor.assumeIsolated {
                guard let self, let json = result as? String, let data = json.data(using: .utf8),
                      var status = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
                else { return }
                self.lastState = status["state"] as? String ?? ""
                if self.lastState == "playing", let spectrum = self.tap.analyse() {
                    status["bins"] = spectrum.bins
                    status["wave"] = spectrum.wave
                }
                status["tap"] = self.tapEnabled ? self.tap.status : "off"
                if let out = try? JSONSerialization.data(withJSONObject: status), let text = String(data: out, encoding: .utf8) {
                    self.onStatus?(text)
                }
            }
        }
    }

    /// toggle, next, prev, seek <s>, volume <0..1>, queue <n>: run by the page script.
    func run(_ name: String, _ arg: Double?) {
        guard ["toggle", "next", "prev", "seek", "volume", "queue"].contains(name) else { return }
        let value = arg.map { String($0) } ?? "undefined"
        webView.evaluateJavaScript("window.__nj && window.__nj.cmd('\(name)', \(value))", completionHandler: nil)
        lastState = "playing" // sample at full rate until the page says otherwise
    }

    /// Editing keys for the sign-in fields, Cmd-W to hide and Cmd-R to reload: only while this
    /// window is in front, so Nightjar's own window keeps its keys.
    private func installShortcuts() {
        keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            let mods = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
            guard mods.contains(.command), let key = event.charactersIgnoringModifiers?.lowercased() else { return event }
            let windowNumber = event.windowNumber
            let handled = MainActor.assumeIsolated { self?.shortcut(key, shift: mods.contains(.shift), windowNumber: windowNumber) ?? false }
            return handled ? nil : event
        }
    }

    /// Returns true when the key was this window's to handle.
    private func shortcut(_ key: String, shift: Bool, windowNumber: Int) -> Bool {
        guard windowNumber == window.windowNumber else { return false }
        switch key {
        case "w":
            window.orderOut(nil)
            return true
        case "r":
            webView.reload()
            return true
        default:
            break
        }
        let action: Selector? = switch key {
        case "x": #selector(NSText.cut(_:))
        case "c": #selector(NSText.copy(_:))
        case "v": #selector(NSText.paste(_:))
        case "a": #selector(NSText.selectAll(_:))
        case "z": shift ? Selector(("redo:")) : Selector(("undo:"))
        default: nil
        }
        guard let action else { return false }
        return NSApp.sendAction(action, to: nil, from: webView)
    }

    // MARK: NSWindowDelegate

    /// Closing hides the window so playback continues.
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        sender.orderOut(nil)
        return false
    }

    // MARK: WKUIDelegate

    /// Links that open a new window (sign-in pop-ups, "open in new tab") load here instead.
    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        if let url = navigationAction.request.url { webView.load(URLRequest(url: url)) }
        return nil
    }
}
