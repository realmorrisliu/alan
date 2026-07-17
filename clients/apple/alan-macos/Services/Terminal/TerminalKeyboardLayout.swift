#if os(macOS)
import Carbon

enum AlanKeyboardLayout {
    static var currentID: String? {
        guard let source = TISCopyCurrentKeyboardInputSource()?.takeRetainedValue(),
              let sourceIDPointer = TISGetInputSourceProperty(source, kTISPropertyInputSourceID)
        else {
            return nil
        }

        let sourceID = unsafeBitCast(sourceIDPointer, to: CFString.self)
        return sourceID as String
    }
}

#endif
