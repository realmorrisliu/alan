import AppKit
import CoreGraphics
import Darwin
import Foundation
import ScreenCaptureKit

struct CaptureOptions {
    var bundleID = "app.alanworks.macos.dev"
    var processID: pid_t?
    var titleContains: String?
    var outputPath: String?
    var listOnly = false
}

enum CaptureChannel: String {
    case stable
    case dev

    var bundleID: String {
        switch self {
        case .stable:
            return "app.alanworks.macos"
        case .dev:
            return "app.alanworks.macos.dev"
        }
    }
}

enum CaptureAlanWindow {
    static func run(options: CaptureOptions) async throws {
        await MainActor.run {
            _ = NSApplication.shared
        }

        let content: SCShareableContent

        do {
            content = try await SCShareableContent.current
        } catch {
            throw NSError(
                domain: "AlanCapture",
                code: 1,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "Unable to access shareable screen content. Check Screen Recording permission for your terminal."
                ]
            )
        }

        let windows = matchingWindows(from: content, options: options)

        guard !windows.isEmpty else {
            throw NSError(
                domain: "AlanCapture",
                code: 2,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "No matching alan windows were found."
                ]
            )
        }

        if options.listOnly {
            for window in windows {
                let app = window.owningApplication
                let title = window.title ?? "<untitled>"
                print(
                    "\(window.windowID)\tpid=\(app?.processID ?? 0)\tbundle=\(app?.bundleIdentifier ?? "-")\tactive=\(window.isActive)\tframe=\(format(window.frame))\t\(title)"
                )
            }
            return
        }

        guard let outputPath = options.outputPath else {
            throw NSError(
                domain: "AlanCapture",
                code: 3,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "Missing required --output path."
                ]
            )
        }

        let window = windows[0]
        let filter = SCContentFilter(desktopIndependentWindow: window)
        let info = SCShareableContent.info(for: filter)
        let scale = max(Double(info.pointPixelScale), 1.0)

        let config = SCStreamConfiguration()
        config.width = max(size_t(window.frame.width * scale), 1)
        config.height = max(size_t(window.frame.height * scale), 1)
        config.showsCursor = false

        let image: CGImage
        do {
            image = try await SCScreenshotManager.captureImage(
                contentFilter: filter,
                configuration: config
            )
        } catch {
            image = try captureImageWithCoreGraphics(windowID: window.windowID)
        }

        guard !imageIsNearBlack(image) else {
            throw NSError(
                domain: "AlanCapture",
                code: 10,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "Captured alan window was blank or near-black, so it cannot be used as visual evidence."
                ]
            )
        }

        let outputURL = URL(fileURLWithPath: outputPath)
        try FileManager.default.createDirectory(
            at: outputURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )

        let representation = NSBitmapImageRep(cgImage: image)
        guard let pngData = representation.representation(using: .png, properties: [:]) else {
            throw NSError(
                domain: "AlanCapture",
                code: 4,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "Failed to encode PNG screenshot."
                ]
            )
        }

        try pngData.write(to: outputURL)
        print(
            "captured \(window.windowID)\t\(window.title ?? "<untitled>")\t-> \(outputURL.path)"
        )
    }

    static func matchingWindows(
        from content: SCShareableContent,
        options: CaptureOptions
    ) -> [SCWindow] {
        content.windows
            .filter { window in
                guard window.windowLayer == 0 else { return false }
                guard window.isOnScreen else { return false }
                guard window.frame.width >= 240, window.frame.height >= 180 else { return false }

                let app = window.owningApplication

                if let processID = options.processID, app?.processID != processID {
                    return false
                }

                if options.processID == nil, app?.bundleIdentifier != options.bundleID {
                    return false
                }

                if let titleContains = options.titleContains?.lowercased(),
                   !(window.title?.lowercased().contains(titleContains) ?? false)
                {
                    return false
                }

                return true
            }
            .sorted(by: compareWindows)
    }

    static func captureImageWithCoreGraphics(windowID: UInt32) throws -> CGImage {
        let options = CGWindowImageOption(arrayLiteral: .boundsIgnoreFraming)
        typealias CreateImageFunction = @convention(c) (
            CGRect,
            UInt32,
            UInt32,
            UInt32
        ) -> Unmanaged<CGImage>?

        let frameworkPath = "/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics"
        guard let handle = dlopen(frameworkPath, RTLD_NOW) else {
            throw NSError(
                domain: "AlanCapture",
                code: 9,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "Failed to capture matching alan window with ScreenCaptureKit or CoreGraphics."
                ]
            )
        }
        defer { dlclose(handle) }

        guard let symbol = dlsym(handle, "CGWindowListCreateImage") else {
            throw NSError(
                domain: "AlanCapture",
                code: 9,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "Failed to capture matching alan window with ScreenCaptureKit or CoreGraphics."
                ]
            )
        }

        let createImage = unsafeBitCast(symbol, to: CreateImageFunction.self)
        guard let image = createImage(
            .null,
            CGWindowListOption.optionIncludingWindow.rawValue,
            windowID,
            options.rawValue
        )?.takeRetainedValue() else {
            throw NSError(
                domain: "AlanCapture",
                code: 9,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "Failed to capture matching alan window with ScreenCaptureKit or CoreGraphics."
                ]
            )
        }

        return image
    }

    static func imageIsNearBlack(_ image: CGImage) -> Bool {
        let sampleWidth = 16
        let sampleHeight = 16
        let bytesPerPixel = 4
        let bytesPerRow = sampleWidth * bytesPerPixel
        var pixels = [UInt8](repeating: 0, count: bytesPerRow * sampleHeight)
        let bitmapInfo = CGImageAlphaInfo.premultipliedLast.rawValue
            | CGBitmapInfo.byteOrder32Big.rawValue

        guard let context = CGContext(
            data: &pixels,
            width: sampleWidth,
            height: sampleHeight,
            bitsPerComponent: 8,
            bytesPerRow: bytesPerRow,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: bitmapInfo
        ) else {
            return false
        }

        context.interpolationQuality = .low
        context.draw(image, in: CGRect(x: 0, y: 0, width: sampleWidth, height: sampleHeight))

        var visibleSamples = 0
        for offset in stride(from: 0, to: pixels.count, by: bytesPerPixel) {
            let brightness = Int(pixels[offset]) + Int(pixels[offset + 1]) + Int(pixels[offset + 2])
            if brightness > 24 {
                visibleSamples += 1
                if visibleSamples >= 3 {
                    return false
                }
            }
        }

        return true
    }

    static func compareWindows(_ lhs: SCWindow, _ rhs: SCWindow) -> Bool {
        if lhs.isActive != rhs.isActive {
            return lhs.isActive && !rhs.isActive
        }

        let lhsArea = lhs.frame.width * lhs.frame.height
        let rhsArea = rhs.frame.width * rhs.frame.height
        if lhsArea != rhsArea {
            return lhsArea > rhsArea
        }

        return lhs.windowID > rhs.windowID
    }

    static func parseOptions(arguments: [String]) throws -> CaptureOptions {
        var options = CaptureOptions()
        var index = 0

        while index < arguments.count {
            let argument = arguments[index]

            switch argument {
            case "--channel":
                index += 1
                let rawValue = try value(after: argument, at: index, in: arguments)
                guard let channel = CaptureChannel(rawValue: rawValue) else {
                    throw NSError(
                        domain: "AlanCapture",
                        code: 8,
                        userInfo: [NSLocalizedDescriptionKey: "Invalid channel: \(rawValue)"]
                    )
                }
                options.bundleID = channel.bundleID
            case "--bundle-id":
                index += 1
                options.bundleID = try value(after: argument, at: index, in: arguments)
            case "--pid":
                index += 1
                let rawValue = try value(after: argument, at: index, in: arguments)
                guard let processID = Int32(rawValue) else {
                    throw NSError(
                        domain: "AlanCapture",
                        code: 5,
                        userInfo: [NSLocalizedDescriptionKey: "Invalid pid: \(rawValue)"]
                    )
                }
                options.processID = processID
            case "--title-contains":
                index += 1
                options.titleContains = try value(after: argument, at: index, in: arguments)
            case "--output":
                index += 1
                options.outputPath = try value(after: argument, at: index, in: arguments)
            case "--list":
                options.listOnly = true
            case "--help", "-h":
                printUsage()
                exit(0)
            default:
                throw NSError(
                    domain: "AlanCapture",
                    code: 6,
                    userInfo: [NSLocalizedDescriptionKey: "Unknown argument: \(argument)"]
                )
            }

            index += 1
        }

        return options
    }

    static func value(after flag: String, at index: Int, in arguments: [String]) throws -> String {
        guard index < arguments.count else {
            throw NSError(
                domain: "AlanCapture",
                code: 7,
                userInfo: [NSLocalizedDescriptionKey: "Missing value after \(flag)"]
            )
        }

        return arguments[index]
    }

    static func format(_ rect: CGRect) -> String {
        "\(Int(rect.origin.x)),\(Int(rect.origin.y)),\(Int(rect.width)),\(Int(rect.height))"
    }

    static func printUsage() {
        print(
            """
            Usage:
              ./clients/apple/scripts/capture-alan-window.swift --output /tmp/alan.png
              ./clients/apple/scripts/capture-alan-window.swift --pid 12345 --output /tmp/alan.png
              ./clients/apple/scripts/capture-alan-window.swift --list

            Options:
              --output <path>            Write PNG screenshot to this path.
              --channel <stable|dev>     Match Alan or Alan Dev by bundle id. Default: dev
              --bundle-id <bundle_id>    Bundle identifier to match. Default: app.alanworks.macos.dev
              --pid <pid>                Match a specific process ID instead of bundle id.
              --title-contains <text>    Match only windows whose title contains this text.
              --list                     Print matching windows without capturing.
              --help                     Show this help.
            """
        )
    }
}

Task {
    do {
        let options = try CaptureAlanWindow.parseOptions(arguments: Array(CommandLine.arguments.dropFirst()))
        try await CaptureAlanWindow.run(options: options)
        exit(0)
    } catch {
        fputs("capture-alan-window: \(error.localizedDescription)\n", stderr)
        exit(1)
    }
}

RunLoop.main.run()
