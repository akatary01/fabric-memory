import AppKit

final class Fixture: NSObject, NSApplicationDelegate {
    private let field = NSTextField(frame: NSRect(x: 24, y: 83, width: 370, height: 28))
    private let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 420, height: 150),
        styleMask: [.titled, .closable], backing: .buffered, defer: false
    )

    func applicationDidFinishLaunching(_ notification: Notification) {
        window.title = "Fabric Computer Use Fixture"
        field.setAccessibilityLabel("Fixture input")
        let button = NSButton(title: "Submit fixture", target: self, action: #selector(submit))
        button.frame = NSRect(x: 24, y: 27, width: 160, height: 34)
        window.contentView?.addSubview(field)
        window.contentView?.addSubview(button)
        window.center()
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(field)
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc private func submit() {
        window.makeFirstResponder(nil)
        let file = Bundle.main.bundleURL.deletingLastPathComponent().appendingPathComponent("submitted.txt")
        try? field.stringValue.write(to: file, atomically: true, encoding: .utf8)
        field.stringValue = ""
        window.makeFirstResponder(field)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

let app = NSApplication.shared
let fixture = Fixture()
app.delegate = fixture
app.setActivationPolicy(.regular)
app.run()
