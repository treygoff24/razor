// swift-tools-version:6.0
// Razor's network client for Apple apps. scripts/apple-network.sh puts this beside Sources/ and the
// RazorNetworkFFI.xcframework it builds; an app adds that directory as a local package.
import PackageDescription

let package = Package(
    name: "RazorNetwork",
    platforms: [.iOS("27.0"), .macCatalyst("27.0"), .macOS("27.0")],
    products: [.library(name: "RazorNetwork", targets: ["RazorNetwork"])],
    targets: [
        .binaryTarget(name: "RazorNetworkFFI", path: "RazorNetworkFFI.xcframework"),
        .target(name: "RazorNetwork", dependencies: ["RazorNetworkFFI"],
                // What rustc's --print native-static-libs lists for the library, less what every
                // Apple program links already, so an app links nothing by hand. Network is the
                // phone's alone; linking it on the Mac costs nothing.
                linkerSettings: ["CoreFoundation", "Network", "Security", "SystemConfiguration"].map { .linkedFramework($0) }
                    + [.linkedLibrary("iconv")]),
    ]
)
