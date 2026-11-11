import AppKit
import WebKit

/// The YouTube Music window: music.youtube.com in a WKWebView with the Nightjar script injected,
/// sampled 30 times a second while playing (twice a second otherwise) and reported to Nightjar
/// with the spectrum from the app's audio tap.
/// Closing the window hides it; music keeps playing. Cmd-Q quits.
@MainActor
final class Player: NSObject, NSWindowDelegate, WKNavigationDelegate, WKUIDelegate {
    static let home = URL(string: "https://music.youtube.com/")!
    /// Google refuses sign-in from browsers it doesn't recognise; present as Safari.
    static let userAgent =
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Safari/605.1.15"

    let window: NSWindow
    let webView: WKWebView
    private let link: Link
    private let tap: AudioTap
    private let tapEnabled: Bool
    /// Tap set-up can wait on the permission prompt; never on the main thread.
    private let tapQueue = DispatchQueue(label: "nightjar.youtube.tap")
    private var timer: Timer?
    private var tick = 0
    private var lastState = ""
    private var activity: NSObjectProtocol?

    init(start: URL, silent: Bool) {
        let config = WKWebViewConfiguration()
        // Persistent, so you sign in once; test runs get a throwaway store and never touch it.
        config.websiteDataStore = silent ? .nonPersistent() : .default()
        // Silent test runs never autoplay; their tap is muted as well.
        config.mediaTypesRequiringUserActionForPlayback = silent ? .all : []
        let scripts = config.userContentController
        if let url = Bundle.main.url(forResource: "inject", withExtension: "js"), let js = try? String(contentsOf: url, encoding: .utf8) {
            scripts.addUserScript(WKUserScript(source: js, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        }

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

        link = Link()
        tap = AudioTap(muted: silent)
        // Silent test runs make no tap unless asked (NJYT_TAP=1): a tap asks for permission.
        tapEnabled = !silent || ProcessInfo.processInfo.environment["NJYT_TAP"] == "1"
        super.init()
        window.delegate = self
        webView.navigationDelegate = self
        webView.uiDelegate = self
        link.onCommand = { [weak self] name, arg in self?.run(name, arg) }

        // App Nap would slow the sampling timer while the window is hidden.
        activity = ProcessInfo.processInfo.beginActivity(options: .userInitiatedAllowingIdleSystemSleep, reason: "Reports playback to Nightjar")
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
                status["tap"] = self.tap.status
                if let out = try? JSONSerialization.data(withJSONObject: status), let text = String(data: out, encoding: .utf8) {
                    self.link.send(text)
                }
            }
        }
    }

    private func run(_ name: String, _ arg: Double?) {
        if name == "show" { return show() }
        guard ["toggle", "next", "prev", "seek", "volume"].contains(name) else { return }
        let value = arg.map { String($0) } ?? "undefined"
        webView.evaluateJavaScript("window.__nj && window.__nj.cmd('\(name)', \(value))", completionHandler: nil)
        lastState = "playing" // sample at full rate until the page says otherwise
    }

    /// Test aid: with `NJYT_SNAP=<file.png>` set at launch, saves what the page shows every 5 s.
    func snapshotEvery5s(to path: String) {
        let timer = Timer(timeInterval: 5, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.webView.takeSnapshot(with: nil) { image, _ in
                    guard let tiff = image?.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff),
                          let png = rep.representation(using: .png, properties: [:]) else { return }
                    try? png.write(to: URL(fileURLWithPath: path))
                }
            }
        }
        RunLoop.main.add(timer, forMode: .common)
    }

    func reload() { webView.reload() }
    func goHome() { webView.load(URLRequest(url: Self.home)) }

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
