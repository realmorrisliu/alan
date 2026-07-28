import Foundation

struct AlanTerminalShellLaunch: Equatable {
    let argumentZero: String
    let arguments: [String]
    let environment: [String: String]

    static func integratingGhostty(
        executablePath: String,
        arguments: [String],
        environment: [String: String],
        resourcesPath: String?,
        fileManager: FileManager = .default
    ) -> AlanTerminalShellLaunch {
        let unchanged = AlanTerminalShellLaunch(
            argumentZero: executablePath,
            arguments: arguments,
            environment: environment
        )
        guard let resourcesPath,
              !resourcesPath.isEmpty
        else {
            return unchanged
        }

        switch URL(fileURLWithPath: executablePath).lastPathComponent {
        case "zsh":
            let integrationDirectory = "\(resourcesPath)/shell-integration/zsh"
            guard fileManager.fileExists(
                atPath: "\(integrationDirectory)/.zshenv"
            ) else {
                return unchanged
            }
            var integratedEnvironment = environment
            integratedEnvironment["GHOSTTY_RESOURCES_DIR"] = resourcesPath
            if let existingZdotdir = environment["ZDOTDIR"] {
                integratedEnvironment["GHOSTTY_ZSH_ZDOTDIR"] = existingZdotdir
            }
            integratedEnvironment["ZDOTDIR"] = integrationDirectory
            return AlanTerminalShellLaunch(
                argumentZero: executablePath,
                arguments: arguments,
                environment: integratedEnvironment
            )
        case "bash":
            return integratingGhosttyBash(
                arguments: arguments,
                environment: environment,
                executablePath: executablePath,
                resourcesPath: resourcesPath,
                fileManager: fileManager
            )
        case "fish":
            let integrationRoot = "\(resourcesPath)/shell-integration"
            guard fileManager.fileExists(
                atPath: "\(integrationRoot)/fish/vendor_conf.d/ghostty-shell-integration.fish"
            ) else {
                return unchanged
            }
            var integratedEnvironment = environment
            integratedEnvironment["GHOSTTY_RESOURCES_DIR"] = resourcesPath
            integratedEnvironment["XDG_DATA_DIRS"] = [
                integrationRoot,
                environment["XDG_DATA_DIRS"],
            ].compactMap { $0 }.joined(separator: ":")
            return AlanTerminalShellLaunch(
                argumentZero: executablePath,
                arguments: arguments,
                environment: integratedEnvironment
            )
        default:
            return unchanged
        }
    }

    private static func integratingGhosttyBash(
        arguments: [String],
        environment: [String: String],
        executablePath: String,
        resourcesPath: String,
        fileManager: FileManager
    ) -> AlanTerminalShellLaunch {
        let unchanged = AlanTerminalShellLaunch(
            argumentZero: executablePath,
            arguments: arguments,
            environment: environment
        )
        let scriptPath = "\(resourcesPath)/shell-integration/bash/ghostty.bash"
        guard fileManager.fileExists(atPath: scriptPath),
              !arguments.contains(where: {
                  $0.hasPrefix("-")
                      && !$0.hasPrefix("--")
                      && $0.dropFirst().contains("c")
              })
        else {
            return unchanged
        }

        var injectionArguments = ["1"]
        var rcfile: String?
        var retainedArguments: [String] = []
        var requestsLoginShell = false
        var suppressesProfile = false
        var index = 0
        while index < arguments.count {
            let argument = arguments[index]
            if argument == "-l" || argument == "--login" {
                requestsLoginShell = true
            } else if argument == "--noprofile" {
                injectionArguments.append(argument)
                suppressesProfile = true
            } else if argument == "--norc" {
                injectionArguments.append(argument)
            } else if argument == "--rcfile" || argument == "--init-file" {
                rcfile = arguments.indices.contains(index + 1)
                    ? arguments[index + 1]
                    : nil
                index += 1
            } else {
                retainedArguments.append(argument)
            }
            index += 1
        }
        if requestsLoginShell, !suppressesProfile {
            // Bash loads the login profile before the injected rcfile. Tell the
            // Ghostty script not to load it a second time.
            injectionArguments.append("--noprofile")
        }
        let integratedArguments =
            (suppressesProfile ? ["--noprofile"] : [])
            + ["--rcfile", scriptPath]
            + retainedArguments

        var integratedEnvironment = environment
        integratedEnvironment["GHOSTTY_RESOURCES_DIR"] = resourcesPath
        integratedEnvironment["GHOSTTY_BASH_INJECT"] =
            injectionArguments.joined(separator: " ")
        if let rcfile {
            integratedEnvironment["GHOSTTY_BASH_RCFILE"] = rcfile
        }
        return AlanTerminalShellLaunch(
            argumentZero: requestsLoginShell
                ? "-\(URL(fileURLWithPath: executablePath).lastPathComponent)"
                : executablePath,
            arguments: integratedArguments,
            environment: integratedEnvironment
        )
    }
}
