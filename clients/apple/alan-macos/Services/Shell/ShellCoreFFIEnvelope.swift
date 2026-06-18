import Foundation

extension ShellCoreFFIAdapter {
    func send<Input: Encodable, Output: Decodable>(
        operation: String,
        payload: Input
    ) throws -> Output {
        let payloadObject = try Self.jsonObject(from: payload, encoder: encoder)
        let request: [String: Any] = [
            "schema_version": ["major": 1, "minor": 0],
            "id": UUID().uuidString.lowercased(),
            "operation": operation,
            "payload": payloadObject,
        ]
        let requestData = try JSONSerialization.data(withJSONObject: request)
        let responseData = try requestData.withUnsafeBytes { requestBytes -> Data in
            let baseAddress = requestBytes.bindMemory(to: UInt8.self).baseAddress
            var responsePointer: UnsafeMutablePointer<UInt8>?
            var responseLength = 0
            let handled = handleRequestFunction(
                baseAddress,
                requestData.count,
                &responsePointer,
                &responseLength
            )
            guard handled != 0 else {
                throw ShellCoreFFIAdapterError.requestFailed
            }
            defer { freeBytesFunction(responsePointer, responseLength) }
            guard let pointer = responsePointer else {
                throw ShellCoreFFIAdapterError.nullResponseBuffer
            }
            return Data(bytes: pointer, count: responseLength)
        }

        let response = try decoder.decode(ShellCoreResponseEnvelope.self, from: responseData)
        if let error = response.error {
            throw ShellCoreFFIAdapterError.facadeError(error)
        }
        guard let payload = response.payload else {
            throw ShellCoreFFIAdapterError.missingPayload(operation)
        }
        return try decoder.decode(Output.self, from: payload)
    }

    static func jsonObject<T: Encodable>(
        from value: T,
        encoder: JSONEncoder
    ) throws -> Any {
        let data = try encoder.encode(value)
        return try JSONSerialization.jsonObject(with: data)
    }


}

enum ShellCoreFFIAdapterError: Error, CustomStringConvertible {
    case libraryLoadFailed(String, String)
    case symbolMissing(String, String)
    case abiVersionMismatch(expected: UInt32, actual: UInt32)
    case requestFailed
    case nullResponseBuffer
    case facadeError(ShellCoreErrorPayload)
    case missingPayload(String)
    case materializationFailed(String)
    case reducerError(code: String, message: String)

    var description: String {
        switch self {
        case .libraryLoadFailed(let path, let message):
            return "failed to load shell core FFI library at \(path): \(message)"
        case .symbolMissing(let name, let message):
            return "missing shell core FFI symbol \(name): \(message)"
        case .abiVersionMismatch(let expected, let actual):
            return "shell core FFI ABI version mismatch: expected \(expected), got \(actual)"
        case .requestFailed:
            return "shell core FFI request failed before producing a response buffer"
        case .nullResponseBuffer:
            return "shell core FFI returned a null response buffer"
        case .facadeError(let error):
            return "shell core FFI \(error.code): \(error.message)"
        case .missingPayload(let operation):
            return "shell core FFI operation \(operation) returned neither payload nor error"
        case .materializationFailed(let message):
            return "shell core FFI materialization failed: \(message)"
        case .reducerError(let code, let message):
            return "shell core FFI reducer \(code): \(message)"
        }
    }
}

struct ShellCoreResponseEnvelope: Decodable {
    let payload: Data?
    let error: ShellCoreErrorPayload?

    private enum CodingKeys: String, CodingKey {
        case payload
        case error
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        error = try container.decodeIfPresent(ShellCoreErrorPayload.self, forKey: .error)
        if container.contains(.payload),
           !(try container.decodeNil(forKey: .payload)) {
            let rawPayload = try container.decode(RawJSONValue.self, forKey: .payload)
            payload = try JSONSerialization.data(withJSONObject: rawPayload.value)
        } else {
            payload = nil
        }
    }
}

#if ALAN_SHELL_CORE_FFI_TESTING
extension ShellCoreFFIAdapter {
    func testingSend<Input: Encodable, Output: Decodable>(
        operation: String,
        payload: Input,
        as _: Output.Type
    ) throws -> Output {
        try send(operation: operation, payload: payload)
    }
}
#endif

struct ShellCoreErrorPayload: Decodable {
    let code: String
    let message: String
}

private struct RawJSONValue: Decodable {
    let value: Any

    init(from decoder: Decoder) throws {
        if let container = try? decoder.singleValueContainer() {
            if container.decodeNil() {
                value = NSNull()
                return
            }
            if let bool = try? container.decode(Bool.self) {
                value = bool
                return
            }
            if let int = try? container.decode(Int.self) {
                value = int
                return
            }
            if let double = try? container.decode(Double.self) {
                value = double
                return
            }
            if let string = try? container.decode(String.self) {
                value = string
                return
            }
        }
        if var array = try? decoder.unkeyedContainer() {
            var values: [Any] = []
            while !array.isAtEnd {
                values.append(try array.decode(RawJSONValue.self).value)
            }
            value = values
            return
        }
        let object = try decoder.container(keyedBy: DynamicCodingKey.self)
        var values: [String: Any] = [:]
        for key in object.allKeys {
            values[key.stringValue] = try object.decode(RawJSONValue.self, forKey: key).value
        }
        value = values
    }
}

private struct DynamicCodingKey: CodingKey {
    let stringValue: String
    let intValue: Int?

    init?(stringValue: String) {
        self.stringValue = stringValue
        intValue = nil
    }

    init?(intValue: Int) {
        stringValue = String(intValue)
        self.intValue = intValue
    }
}
