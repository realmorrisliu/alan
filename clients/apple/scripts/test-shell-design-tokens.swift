import SwiftUI

@main
struct ShellDesignTokenTestRunner {
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

        // Type scale: integer roles, two tracks.
        expect(ShellType.display == 17, "ShellType.display is 17")
        expect(ShellType.heading == 13, "ShellType.heading is 13")
        expect(ShellType.row == 12, "ShellType.row is 12")
        expect(ShellType.caption == 11, "ShellType.caption is 11")
        expect(ShellType.monoLabel == 11, "ShellType.monoLabel is 11")
        expect(ShellType.monoCaption == 10, "ShellType.monoCaption is 10")

        // Spacing scale: 4pt-derived semantic steps.
        expect(ShellSpacing.hair == 2, "ShellSpacing.hair is 2")
        expect(ShellSpacing.tight == 4, "ShellSpacing.tight is 4")
        expect(ShellSpacing.control == 8, "ShellSpacing.control is 8")
        expect(ShellSpacing.row == 12, "ShellSpacing.row is 12")
        expect(ShellSpacing.section == 16, "ShellSpacing.section is 16")
        expect(ShellSpacing.panel == 24, "ShellSpacing.panel is 24")

        // Lamp hierarchy: in dark appearance, every paper surface sits below the
        // ink surface in relative luminance.
        let inkDark = ShellSurfaceValues.luminance(ShellSurfaceValues.inkSurfaceDark)
        for (name, paper) in ShellSurfaceValues.darkPaperSurfaces {
            expect(
                ShellSurfaceValues.luminance(paper) < inkDark,
                "lamp: \(name) sits below ink surface in dark mode"
            )
        }

        // Daylight hierarchy: ink stays far darker than paper in light appearance.
        let inkLight = ShellSurfaceValues.luminance(ShellSurfaceValues.inkSurfaceLight)
        for (name, paper) in ShellSurfaceValues.lightPaperSurfaces {
            expect(
                ShellSurfaceValues.luminance(paper) > inkLight,
                "ink well: \(name) sits above ink surface in light mode"
            )
        }

        if failures > 0 {
            print("\(failures) shell design token check(s) failed")
            exit(1)
        }
        print("All shell design token checks passed")
    }
}
