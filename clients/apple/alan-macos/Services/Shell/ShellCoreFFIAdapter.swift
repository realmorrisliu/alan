import Foundation

struct ShellCoreFFIAdapter {
    let libraryHandle: UnsafeMutableRawPointer
    let abiVersionFunction: ShellCoreABIVersionFunction
    let handleRequestFunction: ShellCoreHandleRequestOutFunction
    let freeBytesFunction: ShellCoreFreeBytesFunction
    let encoder: JSONEncoder
    let decoder: JSONDecoder

    static let iso8601Formatter = ISO8601DateFormatter()
}
