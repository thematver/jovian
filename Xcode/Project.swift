import ProjectDescription

let team = "DTLLQRN8LM"
let version = "1.0.0"

let signing: SettingsDictionary = [
    "DEVELOPMENT_TEAM": .string(team),
    "CODE_SIGN_STYLE": "Manual",
    "CODE_SIGN_IDENTITY": "Apple Development",
    "MACOSX_DEPLOYMENT_TARGET": "14.0",
    "SWIFT_VERSION": "5.0",
    "MARKETING_VERSION": .string(version),
    "CURRENT_PROJECT_VERSION": "1",
    "ENABLE_HARDENED_RUNTIME": "YES",
    "SWIFT_ENFORCE_EXCLUSIVE_ACCESS": "debug-only",
]

let project = Project(
    name: "Jovian8",
    packages: [.local(path: "..")],
    settings: .settings(base: signing),
    targets: [
        .target(
            name: "Jovian8",
            destinations: .macOS,
            product: .app,
            bundleId: "com.anomatver.Jovian8",
            deploymentTargets: .macOS("14.0"),
            infoPlist: .extendingDefault(with: [
                "CFBundleDisplayName": "Jovian-8",
                "LSApplicationCategoryType": "public.app-category.music",
                "NSHumanReadableCopyright": "Jovian-8. Jupiter-8 is a trademark of Roland; Jup-8 V of Arturia. Not affiliated.",
            ]),
            sources: ["App/**"],
            entitlements: .file(path: "App/Jovian8.entitlements"),
            dependencies: [
                .target(name: "Jovian8AU"),
                .package(product: "Jovian8Core"),
            ]
        ),
        .target(
            name: "Jovian8AU",
            destinations: .macOS,
            product: .appExtension,
            bundleId: "com.anomatver.Jovian8.AU",
            deploymentTargets: .macOS("14.0"),
            infoPlist: .extendingDefault(with: [
                "CFBundleDisplayName": "Jovian-8",
                "NSExtension": [
                    "NSExtensionPointIdentifier": "com.apple.AudioUnit-UI",
                    "NSExtensionPrincipalClass": "$(PRODUCT_MODULE_NAME).AudioUnitViewController",
                    "NSExtensionAttributes": [
                        "AudioComponents": [
                            [
                                "name": "Jovian: Jovian-8",
                                "description": "Jovian-8 polyphonic synthesizer",
                                "manufacturer": "Jovn",
                                "type": "aumu",
                                "subtype": "JV8e",
                                "version": 65536,
                                "sandboxSafe": true,
                                "tags": ["Synthesizer"],
                                "factoryFunction": "$(PRODUCT_MODULE_NAME).AudioUnitViewController",
                            ],
                        ],
                    ],
                ],
            ]),
            sources: ["AU/**"],
            entitlements: .file(path: "AU/Jovian8AU.entitlements"),
            dependencies: [.package(product: "Jovian8Core")]
        ),
    ]
)
