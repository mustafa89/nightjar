import Foundation

/// UDP on 127.0.0.1 only: status JSON out to Nightjar (port 47813), commands in (port 47814) as
/// plain text, `name` or `name value` (toggle, next, prev, show, seek 42.5, volume 0.6).
@MainActor
final class Link {
    static let statusPort: UInt16 = 47813
    static let commandPort: UInt16 = 47814

    var onCommand: ((String, Double?) -> Void)?
    private var socket: Int32 = -1
    private var source: DispatchSourceRead?

    init() {
        let fd = Darwin.socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP)
        guard fd >= 0 else { return }
        var addr = Self.loopback(port: Self.commandPort)
        let bound = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size)) }
        }
        guard bound == 0 else {
            close(fd)
            return
        }
        _ = fcntl(fd, F_SETFL, O_NONBLOCK)
        socket = fd
        let source = DispatchSource.makeReadSource(fileDescriptor: fd, queue: .main)
        source.setEventHandler { [weak self] in
            MainActor.assumeIsolated { self?.drain() }
        }
        source.setCancelHandler { close(fd) }
        source.resume()
        self.source = source
    }

    private static func loopback(port: UInt16) -> sockaddr_in {
        var addr = sockaddr_in()
        addr.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = port.bigEndian
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")
        return addr
    }

    func send(_ json: String) {
        guard socket >= 0 else { return }
        var addr = Self.loopback(port: Self.statusPort)
        let bytes = Array(json.utf8)
        _ = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                sendto(socket, bytes, bytes.count, 0, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
    }

    private func drain() {
        var buffer = [UInt8](repeating: 0, count: 256)
        while true {
            let n = recv(socket, &buffer, buffer.count, 0)
            if n <= 0 { break }
            let words = String(decoding: buffer[0..<n], as: UTF8.self).split(separator: " ")
            guard let name = words.first else { continue }
            onCommand?(String(name), words.count > 1 ? Double(words[1]) : nil)
        }
    }
}
