import AppKit

// The C surface Rust calls (src/youtube.rs). Every function runs on the main thread: Rune's
// handlers and poll do, and so does AppKit.

public typealias NJWake = @convention(c) (UnsafeMutableRawPointer?) -> Void

@MainActor
private enum Backend {
    static var player: YouTubePlayer?
    /// The newest status JSON and its sequence number (0 = none yet).
    static var status: [UInt8] = []
    static var seq: UInt64 = 0
}

/// Starts the YouTube player once: `script` is the page script, `url` the first page (NULL for
/// YouTube Music's home). The window stays hidden until `nj_yt_show`. `wake(ctx)` runs after
/// each new status. `silent` and `tap` are for tests (see YouTubePlayer.init).
@_cdecl("nj_yt_start")
public func nj_yt_start(_ wake: NJWake?, _ ctx: UnsafeMutableRawPointer?, _ script: UnsafePointer<CChar>, _ url: UnsafePointer<CChar>?, _ silent: Bool, _ tap: Bool) {
    let js = String(cString: script)
    let start = url.flatMap { URL(string: String(cString: $0)) } ?? YouTubePlayer.home
    nonisolated(unsafe) let context = ctx
    MainActor.assumeIsolated {
        guard Backend.player == nil else { return }
        let player = YouTubePlayer(start: start, script: js, silent: silent, tap: tap)
        player.onStatus = { text in
            Backend.status = Array(text.utf8)
            Backend.seq &+= 1
            wake?(context)
        }
        Backend.player = player
    }
}

@_cdecl("nj_yt_show")
public func nj_yt_show() {
    MainActor.assumeIsolated { Backend.player?.show() }
}

/// `name` with an optional number: toggle, next, prev, seek, volume, queue.
@_cdecl("nj_yt_command")
public func nj_yt_command(_ name: UnsafePointer<CChar>, _ arg: Double, _ hasArg: Bool) {
    let command = String(cString: name)
    MainActor.assumeIsolated { Backend.player?.run(command, hasArg ? arg : nil) }
}

/// Copies the newest status into `buf` when it's newer than `after`, sets `seq` to its number
/// and returns its length; returns 0 when there's nothing newer (or it doesn't fit).
@_cdecl("nj_yt_status")
public func nj_yt_status(_ after: UInt64, _ seq: UnsafeMutablePointer<UInt64>, _ buf: UnsafeMutablePointer<UInt8>, _ cap: Int) -> Int {
    MainActor.assumeIsolated {
        guard Backend.seq > after, Backend.status.count <= cap else { return 0 }
        Backend.status.withUnsafeBufferPointer { src in buf.update(from: src.baseAddress!, count: src.count) }
        seq.pointee = Backend.seq
        return Backend.status.count
    }
}
