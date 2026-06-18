import Darwin
import Foundation

private let shellCoreFFIAdapterSharedStorage = ShellCoreFFIAdapterSharedStorage()

typealias ShellCoreABIVersionFunction = @convention(c) () -> UInt32
typealias ShellCoreHandleRequestOutFunction =
    @convention(c) (
        UnsafePointer<UInt8>?,
        Int,
        UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>?,
        UnsafeMutablePointer<Int>?
    ) -> UInt8
typealias ShellCoreFreeBytesFunction =
    @convention(c) (UnsafeMutablePointer<UInt8>?, Int) -> Void

extension ShellCoreFFIAdapter {
    static var shared: ShellCoreFFIAdapter {
        get throws {
            try shellCoreFFIAdapterSharedStorage.adapter()
        }
    }

    init(libraryPath: String? = ProcessInfo.processInfo.environment["ALAN_SHELL_CORE_FFI_LIBRARY"]) throws {
        let resolvedPath = libraryPath ?? Self.bundledLibraryPath()
        guard let libraryHandle = dlopen(resolvedPath, RTLD_NOW | RTLD_LOCAL) else {
            throw ShellCoreFFIAdapterError.libraryLoadFailed(resolvedPath, Self.dlerrorMessage())
        }
        self.libraryHandle = libraryHandle

        abiVersionFunction = try Self.loadSymbol(
            "alan_shell_core_ffi_abi_version",
            from: libraryHandle,
            as: ShellCoreABIVersionFunction.self
        )
        handleRequestFunction = try Self.loadSymbol(
            "alan_shell_core_ffi_handle_request_out",
            from: libraryHandle,
            as: ShellCoreHandleRequestOutFunction.self
        )
        freeBytesFunction = try Self.loadSymbol(
            "alan_shell_core_ffi_free_bytes",
            from: libraryHandle,
            as: ShellCoreFreeBytesFunction.self
        )

        let abiVersion = abiVersionFunction()
        guard abiVersion == Self.expectedABIVersion else {
            throw ShellCoreFFIAdapterError.abiVersionMismatch(
                expected: Self.expectedABIVersion,
                actual: abiVersion
            )
        }

        encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
    }

    private static func loadSymbol<T>(
        _ name: String,
        from handle: UnsafeMutableRawPointer,
        as type: T.Type
    ) throws -> T {
        guard let symbol = dlsym(handle, name) else {
            throw ShellCoreFFIAdapterError.symbolMissing(name, dlerrorMessage())
        }
        return unsafeBitCast(symbol, to: type)
    }

    private static func bundledLibraryPath() -> String {
        Bundle.main.privateFrameworksURL?
            .appendingPathComponent("libalan_shell_core_ffi.dylib")
            .path
            ?? "libalan_shell_core_ffi.dylib"
    }

    private static func dlerrorMessage() -> String {
        dlerror().map { String(cString: $0) } ?? "unknown dynamic linker error"
    }

    private static var expectedABIVersion: UInt32 { 1 }
}

private final class ShellCoreFFIAdapterSharedStorage: @unchecked Sendable {
    private let lock = NSLock()
    private var cachedAdapter: ShellCoreFFIAdapter?

    func adapter() throws -> ShellCoreFFIAdapter {
        lock.lock()
        defer { lock.unlock() }
        if let cachedAdapter {
            return cachedAdapter
        }
        let adapter = try ShellCoreFFIAdapter()
        cachedAdapter = adapter
        return adapter
    }
}
