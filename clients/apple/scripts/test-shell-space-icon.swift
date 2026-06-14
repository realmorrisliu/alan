import CoreGraphics
import Foundation

@main
struct ShellSpaceIconTestRunner {
    static func main() {
        var failures = 0

        func expect(_ condition: Bool, _ label: String) {
            if condition {
                print("PASS \(label)")
            } else {
                failures += 1
                print("FAIL \(label)")
            }
        }

        // MARK: - monogram(forTitle:)

        expect(
            ShellSpacePresentationIcon.monogram(forTitle: "univer") == "U",
            "monogram(\"univer\") == \"U\""
        )
        expect(
            ShellSpacePresentationIcon.monogram(forTitle: "dogfooding") == "D",
            "monogram(\"dogfooding\") == \"D\""
        )
        expect(
            ShellSpacePresentationIcon.monogram(forTitle: "  spaced") == "S",
            "monogram(\"  spaced\") == \"S\" (leading whitespace trimmed)"
        )
        expect(
            ShellSpacePresentationIcon.monogram(forTitle: nil) == "",
            "monogram(nil) == \"\""
        )
        expect(
            ShellSpacePresentationIcon.monogram(forTitle: "") == "",
            "monogram(\"\") == \"\""
        )
        expect(
            ShellSpacePresentationIcon.monogram(forTitle: "   ") == "",
            "monogram(\"   \") == \"\" (whitespace-only)"
        )
        expect(
            ShellSpacePresentationIcon.monogram(forTitle: "中文") == "中",
            "monogram(\"中文\") == \"中\" (first CJK grapheme as-is)"
        )
        expect(
            ShellSpacePresentationIcon.monogram(forTitle: "🚀 rocket") == "🚀",
            "monogram(\"🚀 rocket\") == \"🚀\" (leading emoji grapheme as-is)"
        )

        // MARK: - resolve(systemName:title:)

        let resolveNilSymbolUniver = ShellSpacePresentationIcon.resolve(
            systemName: nil, title: "univer"
        )
        expect(
            resolveNilSymbolUniver == .monogram("U"),
            "resolve(nil, \"univer\") == .monogram(\"U\")"
        )

        let resolveValidSymbol = ShellSpacePresentationIcon.resolve(
            systemName: "hammer", title: "x"
        )
        expect(
            resolveValidSymbol == .symbol("hammer"),
            "resolve(\"hammer\", \"x\") == .symbol(\"hammer\")"
        )

        // "not a symbol!!" contains spaces and "!" — isSupportedSystemName must reject it,
        // so resolution falls through to the title monogram.
        let resolveInvalidSymbol = ShellSpacePresentationIcon.resolve(
            systemName: "not a symbol!!", title: "x"
        )
        expect(
            resolveInvalidSymbol == .monogram("X"),
            "resolve(\"not a symbol!!\", \"x\") == .monogram(\"X\") (invalid symbol falls through)"
        )

        let resolveNilNil = ShellSpacePresentationIcon.resolve(
            systemName: nil, title: nil
        )
        expect(
            resolveNilNil == .fallbackSymbol("square.grid.2x2"),
            "resolve(nil, nil) == .fallbackSymbol(\"square.grid.2x2\")"
        )

        // Empty title after trim also falls back to the neutral symbol.
        let resolveNilEmpty = ShellSpacePresentationIcon.resolve(
            systemName: nil, title: "  "
        )
        expect(
            resolveNilEmpty == .fallbackSymbol("square.grid.2x2"),
            "resolve(nil, \"  \") == .fallbackSymbol(\"square.grid.2x2\")"
        )

        // MARK: - ShellSpaceIconCatalog: every curated symbol must pass isSupportedSystemName

        expect(
            !ShellSpaceIconCatalog.curatedSymbols.isEmpty,
            "curated symbol list must be non-empty"
        )
        for name in ShellSpaceIconCatalog.curatedSymbols {
            expect(
                ShellSpacePresentationIcon.isSupportedSystemName(name),
                "curated symbol \"\(name)\" must pass isSupportedSystemName"
            )
        }

        // MARK: - ShellSpaceDefaultName.derive(fromWorkingDirectory:)

        expect(ShellSpaceDefaultName.derive(fromWorkingDirectory: "/Users/x/univer") == "univer",
               "derive: plain leaf")
        expect(ShellSpaceDefaultName.derive(fromWorkingDirectory: "/Users/x/univer/") == "univer",
               "derive: trailing slash")
        expect(ShellSpaceDefaultName.derive(fromWorkingDirectory: "/Users/x/proj/.git") == "proj",
               "derive: strips .git")
        expect(ShellSpaceDefaultName.derive(fromWorkingDirectory: "/") == "",
               "derive: root is empty")
        expect(ShellSpaceDefaultName.derive(fromWorkingDirectory: nil) == "",
               "derive: nil is empty")
        expect(ShellSpaceDefaultName.derive(fromWorkingDirectory: "  ") == "",
               "derive: blank is empty")

        if failures > 0 {
            print("\(failures) shell space icon check(s) failed")
            exit(1)
        }
        print("All shell space icon checks passed.")
    }
}
