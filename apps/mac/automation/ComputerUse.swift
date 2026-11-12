import AppKit
import ApplicationServices
import Foundation
import PDFKit
import ImageIO
import UniformTypeIdentifiers

func emit(_ value:Any){let data=(try? JSONSerialization.data(withJSONObject:value,options:[.sortedKeys])) ?? Data("{}".utf8);FileHandle.standardOutput.write(data);FileHandle.standardOutput.write(Data([10]))}
struct ToolError:Error,CustomStringConvertible{let description:String;init(_ message:String){description=message}}
func attr(_ element:AXUIElement,_ key:String)->CFTypeRef?{var value:CFTypeRef?;guard AXUIElementCopyAttributeValue(element,key as CFString,&value) == .success else{return nil};return value}
func children(_ element:AXUIElement)->[AXUIElement]{attr(element,kAXChildrenAttribute) as? [AXUIElement] ?? []}
func app(_ name:String?)->NSRunningApplication?{
 if let name,!name.isEmpty{let identifier=(try? applicationURL(name)).flatMap{Bundle(url:$0)?.bundleIdentifier} ?? name;return NSWorkspace.shared.runningApplications.first{$0.bundleIdentifier==identifier || $0.localizedName==name}}
 return NSWorkspace.shared.frontmostApplication
}
func applicationURL(_ input:String)throws->URL {
 let value=input.trimmingCharacters(in:.whitespacesAndNewlines)
 if ["browser","web","default browser","web browser"].contains(value.lowercased()) {
  guard let url=NSWorkspace.shared.urlForApplication(toOpen:URL(string:"https://example.com")!)else{throw ToolError("No default browser is configured. Set one in System Settings → Desktop & Dock.")};return url
 }
 let aliases=["chrome":"com.google.Chrome","google chrome":"com.google.Chrome","safari":"com.apple.Safari","firefox":"org.mozilla.firefox","edge":"com.microsoft.edgemac"]
 if let url=NSWorkspace.shared.urlForApplication(withBundleIdentifier:aliases[value.lowercased()] ?? value){return url}
 if let path=NSWorkspace.shared.fullPath(forApplication:value){return URL(fileURLWithPath:path)}
 throw ToolError("Could not find installed app '\(value)'. Use resolve_app with its name, or choose an installed browser.")
}
func resolved(_ input:String)throws->[String:Any]{let url=try applicationURL(input);return ["app":Bundle(url:url)?.bundleIdentifier ?? input,"name":url.deletingPathExtension().lastPathComponent]}
func label(_ element:AXUIElement)->String{let role=attr(element,kAXRoleAttribute) as? String ?? "";let title=attr(element,kAXTitleAttribute) as? String ?? attr(element,kAXDescriptionAttribute) as? String ?? "";let value=(role=="AXSecureTextField" || (attr(element,kAXSubroleAttribute) as? String)=="AXSecureTextField") ? "[secure]":(attr(element,kAXValueAttribute) as? String ?? "");return [role,title,String(value.prefix(250))].filter{!$0.isEmpty}.joined(separator:" · ").components(separatedBy:.newlines).joined(separator:" ")}
func node(_ root:AXUIElement,_ path:String)throws->AXUIElement{var item=root;for part in path.split(separator:"."){guard let index=Int(part),children(item).indices.contains(index)else{throw ToolError("The UI changed. Observe the app again.")};item=children(item)[index]};return item}
func trusted()throws{guard AXIsProcessTrusted()else{throw ToolError("Enable Accessibility for fabric-memory in System Settings before using Mac controls.")}}
func focusedAddressField(_ root: AXUIElement) -> AXUIElement? {
 guard let focused = attr(root, kAXFocusedUIElementAttribute),
       CFGetTypeID(focused) == AXUIElementGetTypeID() else { return nil }
 let field = focused as! AXUIElement
 let role = attr(field, kAXRoleAttribute) as? String
 guard role == "AXTextField" || role == "AXComboBox" else { return nil }
 var ancestor = field
 for _ in 0..<12 {
  guard let parent = attr(ancestor, kAXParentAttribute),
        CFGetTypeID(parent) == AXUIElementGetTypeID() else { break }
  ancestor = parent as! AXUIElement
  let ancestorRole = attr(ancestor, kAXRoleAttribute) as? String
  if ancestorRole == "AXWebArea" { break }
  if ancestorRole == "AXToolbar" { return field }
 }
 return nil
}

func addressMatches(_ displayed: String, _ url: URL) -> Bool {
 let value = displayed.trimmingCharacters(in: .whitespacesAndNewlines)
 if value == url.absoluteString { return true }
 let prefix = (url.scheme ?? "https") + "://"
 let address = value.hasPrefix("https://") || value.hasPrefix("http://") ? value : prefix + value
 guard let observed = URLComponents(string: address),
       let expected = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return false }
 return observed.scheme?.lowercased() == expected.scheme?.lowercased()
   && observed.host?.lowercased() == expected.host?.lowercased()
   && observed.port == expected.port && observed.user == nil && observed.password == nil
   && observed.path == expected.path && observed.queryItems == expected.queryItems
   && observed.fragment == expected.fragment
}

func enterAddress(_ url: URL, root: AXUIElement, target: NSRunningApplication) throws {
 let focusDeadline = Date().addingTimeInterval(1.5)
 var field = focusedAddressField(root)
 while field == nil && target.isActive && Date() < focusDeadline {
  RunLoop.current.run(until: Date().addingTimeInterval(0.05))
  field = focusedAddressField(root)
 }
 guard let initial = field else {
  throw ToolError("Could not identify the browser address field. A blank tab may already be open.")
 }
 // Use the browser's editable address field directly. Some browsers ignore
 // synthetic Unicode keyboard events in their address bar.
 guard target.isActive,
       AXUIElementSetAttributeValue(initial, kAXValueAttribute as CFString, url.absoluteString as CFString) == .success else {
  throw ToolError("The browser did not accept the address through Accessibility.")
 }
 let deadline = Date().addingTimeInterval(2)
 repeat {
  guard target.isActive, let field = focusedAddressField(root), CFEqual(initial, field) else {
   throw ToolError("The browser address field lost focus before navigation.")
  }
  if let value = attr(field, kAXValueAttribute) as? String, addressMatches(value, url) { return }
  RunLoop.current.run(until: Date().addingTimeInterval(0.05))
 } while Date() < deadline
 throw ToolError("Could not confirm the new tab’s address. The address was not submitted.")
}

func windowDetails(_ target: NSRunningApplication) -> [String: String]? {
 let root = AXUIElementCreateApplication(target.processIdentifier)
 guard let value = attr(root, kAXFocusedWindowAttribute),
       CFGetTypeID(value) == AXUIElementGetTypeID() else { return nil }
 let window = value as! AXUIElement
 var details = ["app": target.localizedName ?? target.bundleIdentifier ?? "App"]
 if let title = attr(window, kAXTitleAttribute) as? String, !title.isEmpty {
  details["title"] = title
 }
 let address = (attr(window, kAXURLAttribute) as? URL)?.absoluteString
  ?? (attr(window, kAXURLAttribute) as? String)
  ?? (attr(window, kAXDocumentAttribute) as? String)
 if let address, !address.isEmpty { details["address"] = address }
 return details
}

func attachWindow(_ result: inout [String: Any], target: NSRunningApplication) {
 guard let window = windowDetails(target) else { return }
 result["window"] = window
 if let name = window["app"], let title = window["title"] {
  result["window_title"] = "\(name) — \(title)"
 }
}

func sampleDeviceUsage(at path: String) -> [String: Any] {
    let host = mach_host_self()
    defer { mach_port_deallocate(mach_task_self_, host) }
    func cpuTicks() -> [UInt32]? {
        var load = host_cpu_load_info()
        var count = mach_msg_type_number_t(MemoryLayout<host_cpu_load_info>.size / MemoryLayout<integer_t>.size)
        let status = withUnsafeMutablePointer(to: &load) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                host_statistics(host, HOST_CPU_LOAD_INFO, $0, &count)
            }
        }
        guard status == KERN_SUCCESS else { return nil }
        return [load.cpu_ticks.0, load.cpu_ticks.1, load.cpu_ticks.2, load.cpu_ticks.3]
    }
    var sample: [String: Any] = [:]
    if let start = cpuTicks() {
        Thread.sleep(forTimeInterval: 0.2)
        if let end = cpuTicks() {
            let delta = zip(start, end).map { UInt64($1 &- $0) }
            let total = delta.reduce(0, +)
            if total > 0 {
                sample["cpu_percent"] = 100 * Double(total - delta[Int(CPU_STATE_IDLE)]) / Double(total)
            }
        }
    }
    var vm = vm_statistics64()
    var vmCount = mach_msg_type_number_t(MemoryLayout<vm_statistics64>.size / MemoryLayout<integer_t>.size)
    let vmStatus = withUnsafeMutablePointer(to: &vm) {
        $0.withMemoryRebound(to: integer_t.self, capacity: Int(vmCount)) {
            host_statistics64(host, HOST_VM_INFO64, $0, &vmCount)
        }
    }
    if vmStatus == KERN_SUCCESS {
        let total = ProcessInfo.processInfo.physicalMemory
        let pages = UInt64(vm.active_count) + UInt64(vm.wire_count) + UInt64(vm.compressor_page_count)
        sample["memory_total"] = total
        sample["memory_used"] = min(total, pages * UInt64(getpagesize()))
    }
    if let disk = try? FileManager.default.attributesOfFileSystem(forPath: path),
       let size = disk[.systemSize] as? NSNumber,
       let free = disk[.systemFreeSize] as? NSNumber {
        sample["disk_total"] = size.uint64Value
        sample["disk_used"] = size.uint64Value - min(size.uint64Value, free.uint64Value)
    }
    return sample
}

func run(_ args:[String:Any])throws->[String:Any]{
 let action=args["action"] as? String ?? "observe"
 if action == "resize_vision_image" {
  guard let encoded = args["data"] as? String, let data = Data(base64Encoded: encoded),
        let source = CGImageSourceCreateWithData(data as CFData, nil),
        let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
         kCGImageSourceCreateThumbnailFromImageAlways: true,
         kCGImageSourceThumbnailMaxPixelSize: 1024,
         kCGImageSourceCreateThumbnailWithTransform: true
        ] as CFDictionary) else {
   throw ToolError("Could not prepare the screenshot for the local vision model.")
  }
  let output = NSMutableData()
  guard let destination = CGImageDestinationCreateWithData(output, UTType.jpeg.identifier as CFString, 1, nil) else {
   throw ToolError("Could not encode the vision screenshot.")
  }
  CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality: 0.8] as CFDictionary)
  guard CGImageDestinationFinalize(destination) else { throw ToolError("Could not encode the vision screenshot.") }
  return ["image": (output as Data).base64EncodedString()]
 }
 if action=="extract_pdf_text" {
  guard let encoded=args["data"] as? String, encoded.utf8.count <= 8*1024*1024,
   let data=Data(base64Encoded:encoded), let pdf=PDFDocument(data:data), !pdf.isLocked else {
    throw ToolError("Could not open the PDF. Check whether it is encrypted or damaged.")
   }
  var text=""
  for i in 0..<min(pdf.pageCount,200) {
   if let page=pdf.page(at:i)?.string { text += String(page.prefix(max(0,12001-text.count)))+"\n" }
   if text.count>12000 {break}
  }
  return ["text":text]
 }
 if action == "device_usage" {
  let directory = ProcessInfo.processInfo.environment["FABRIC_DATA_DIR"] ?? NSHomeDirectory()
  return sampleDeviceUsage(at: directory)
 }
 if action=="permissions"{return ["accessibility":AXIsProcessTrusted(),"screen_recording":CGPreflightScreenCaptureAccess()]}
 if action=="request_permissions"{
  if args["control"] as? Bool == true {
   let options=[kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String:true] as CFDictionary
   _=AXIsProcessTrustedWithOptions(options)
  }
  if args["screen"] as? Bool == true && !CGPreflightScreenCaptureAccess(){_=CGRequestScreenCaptureAccess()}
  return ["accessibility":AXIsProcessTrusted(),"screen_recording":CGPreflightScreenCaptureAccess()]
 }
 if action=="trash_workspace_item" {
  guard let path=args["path"] as? String, path.hasPrefix("/") else {throw ToolError("Missing workspace item")}
  try FileManager.default.trashItem(at:URL(fileURLWithPath:path),resultingItemURL:nil)
  return ["trashed":true]
 }
 if action=="resolve_app"{return try resolved(args["app"] as? String ?? "browser")}
 if action=="open_app"{
  let input=args["app"] as? String ?? "browser";let url=try applicationURL(input)
  guard NSWorkspace.shared.open(url)else{throw ToolError("Could not open that app")};var result=try resolved(input);result["opened"]=result["app"];return result
 }
 if action=="search_browser"{
  guard let query=args["query"] as? String,!query.trimmingCharacters(in:.whitespacesAndNewlines).isEmpty,query.utf8.count<=2000 else{throw ToolError("Browser search needs a query up to 2,000 bytes.")}
  var url=URLComponents(string:"https://www.google.com/search")!;url.queryItems=[URLQueryItem(name:"q",value:query)];return try run(["action":"open_url","url":url.url!.absoluteString,
   "new_tab":args["new_tab"] as? Bool ?? false,"app":args["app"] as? String ?? "browser"])
 }
 if action=="open_url"{
  guard let raw=args["url"] as? String,raw.utf8.count<=8192,let url=URL(string:raw),["https","http"].contains(url.scheme?.lowercased() ?? ""),url.host != nil,url.user == nil,url.password == nil else{throw ToolError("Use a complete HTTP or HTTPS URL without embedded credentials.")}
  let browserName = args["app"] as? String ?? "browser"
  let browser = try resolved(browserName)
  let supportedBrowsers: Set<String> = ["com.google.Chrome", "com.google.Chrome.beta",
   "com.google.Chrome.dev", "com.google.Chrome.canary", "com.apple.Safari",
   "org.mozilla.firefox", "com.microsoft.edgemac", "com.brave.Browser"]
  let browserAlias = ["browser","web","default browser","web browser"].contains(browserName.lowercased())
  guard browserAlias || supportedBrowsers.contains(browser["app"] as? String ?? "") else {
   throw ToolError("Choose an installed browser such as Chrome or the default browser.")
  }
  if args["new_tab"] as? Bool == true {
   try trusted()
   guard let identifier = browser["app"] as? String else { throw ToolError("Could not resolve the default browser.") }
   if app(identifier) == nil {
    _ = try run(["action":"open_app","app":identifier])
    let deadline = Date().addingTimeInterval(5)
    while app(identifier) == nil && Date() < deadline {
     RunLoop.current.run(until: Date().addingTimeInterval(0.05))
    }
   }
   _ = try run(["action":"key","app":identifier,"key":"Cmd+T"])
   RunLoop.current.run(until: Date().addingTimeInterval(0.3))
   _ = try run(["action":"key","app":identifier,"key":"Cmd+L"])
   RunLoop.current.run(until: Date().addingTimeInterval(0.15))
   guard let target = app(identifier), target.isActive else { throw ToolError("The browser lost focus before navigation.") }
   let root = AXUIElementCreateApplication(target.processIdentifier)
   try enterAddress(url, root: root, target: target)
   _ = try run(["action":"key","app":identifier,"key":"Return"])
   // Allow Chrome to move focus from the address editor to the new document.
   let deadline = Date().addingTimeInterval(1.5)
   while Date() < deadline {
    if let address = windowDetails(target)?["address"], addressMatches(address, url) { break }
    RunLoop.current.run(until: Date().addingTimeInterval(0.05))
   }
   var result = browser
   attachWindow(&result, target: target)
   if var window = result["window"] as? [String: String],
      let address = window["address"], !addressMatches(address, url) {
    window.removeValue(forKey: "address")
    result["window"] = window
   }
   result["navigated_url"] = url.absoluteString
   result["new_tab"] = true
   result["verification"] = "Entered the address in a new tab and dispatched navigation. Observe the browser next."
   return result
  }
  if browserAlias {
   guard NSWorkspace.shared.open(url) else { throw ToolError("The default browser could not open the URL.") }
  } else {
   let application = try applicationURL(browserName)
   let lock = NSLock()
   var reply: Result<NSRunningApplication, Error>?
   NSWorkspace.shared.open([url], withApplicationAt: application, configuration: .init()) { running, error in
    lock.withLock {
     if let running { reply = .success(running) }
     else { reply = .failure(error ?? ToolError("The selected browser could not open the URL.")) }
    }
   }
   let deadline = Date().addingTimeInterval(5)
   while lock.withLock({ reply == nil }) && Date() < deadline {
    RunLoop.current.run(until: Date().addingTimeInterval(0.05))
   }
   guard let reply = lock.withLock({ reply }) else { throw ToolError("The selected browser did not acknowledge the URL. Observe it before retrying.") }
   _ = try reply.get()
  }
  var result = browser
  result["navigated_url"] = url.absoluteString
  result["verification"] = "Navigation dispatched; observe the browser to verify the loaded page."
  return result
 }
 if action=="screenshot"{
  guard CGPreflightScreenCaptureAccess()else{throw ToolError("Enable Screen Recording for fabric-memory in System Settings.")}
  guard let directory=ProcessInfo.processInfo.environment["FABRIC_DATA_DIR"]else{throw ToolError("Missing app storage")}
  let folder=URL(fileURLWithPath:directory).appendingPathComponent("Captures");try FileManager.default.createDirectory(at:folder,withIntermediateDirectories:true,attributes:[.posixPermissions:0o700]);let file=folder.appendingPathComponent(UUID().uuidString+".png")
  let process=Process();process.executableURL=URL(fileURLWithPath:"/usr/sbin/screencapture");process.arguments=["-x","-m",file.path];try process.run();process.waitUntilExit();guard process.terminationStatus==0 else{throw ToolError("Screenshot failed")};try FileManager.default.setAttributes([.posixPermissions:0o600],ofItemAtPath:file.path)
  return ["image_path":file.path,"summary":"Captured this Mac’s screen"]
 }
 try trusted()
 guard let target=app(args["app"] as? String)else{throw ToolError("App is not running")}
 let root=AXUIElementCreateApplication(target.processIdentifier)
 if action=="observe"{
  // Chromium/Electron publish fuller AX trees when an assistive client requests them.
  // This follows the OS trust check above; it does not grant Accessibility access.
  _=AXUIElementSetAttributeValue(root,"AXEnhancedUserInterface" as CFString,kCFBooleanTrue)
  _=AXUIElementSetAttributeValue(root,"AXManualAccessibility" as CFString,kCFBooleanTrue)
  RunLoop.current.run(until:Date().addingTimeInterval(0.08))
  var lines=[String]();var count=0
  func walk(_ element:AXUIElement,_ path:String,_ depth:Int){guard count<650 && depth<20 else{return};count+=1;lines.append(path+" "+label(element));for (index,child) in children(element).enumerated().sorted(by:{(attr($0.element,kAXRoleAttribute) as? String == "AXWindow" ? 0:1)<(attr($1.element,kAXRoleAttribute) as? String == "AXWindow" ? 0:1)}){walk(child,path.isEmpty ? String(index):path+"."+String(index),depth+1)}}
  walk(root,"",0);return ["app":target.bundleIdentifier ?? target.localizedName ?? "App","pid":target.processIdentifier,"ui":String(lines.joined(separator:"\n").prefix(24000)),"window_title":(attr(root,kAXFocusedWindowAttribute) as! AXUIElement?).map{attr($0,kAXTitleAttribute) as? String ?? ""} ?? ""]
 }
 target.activate(options:[])
 let activationDeadline=Date().addingTimeInterval(1.5)
 while !target.isActive && Date()<activationDeadline{RunLoop.current.run(until:Date().addingTimeInterval(0.02))}
 guard target.isActive else{throw ToolError("The target app could not become active. Observe again before sending input.")}
 if action=="click"{
  if let path=args["element"] as? String{
   guard let expected=args["expected"] as? String,!expected.isEmpty else{throw ToolError("Element clicks require the expected label from a fresh observation.")}
   if let observedPID = args["observed_pid"] as? Int, observedPID != Int(target.processIdentifier) {
    throw ToolError("The target app restarted. Observe it again before clicking.")
   }
   let element=try node(root,path)
   let currentLabel = label(element)
   let matches = (args["expected_exact"] as? Bool == true) ? currentLabel == expected : currentLabel.contains(expected)
   guard matches else {throw ToolError("The target changed. Observe again before clicking.")}
   if args["require_link"] as? Bool == true,
      attr(element, kAXRoleAttribute) as? String != "AXLink" {
    throw ToolError("The observed element is not a link.")
   }
   let clickedLabel = currentLabel
   guard AXUIElementPerformAction(element,kAXPressAction as CFString) == .success else{throw ToolError("This element cannot be pressed; use its screen coordinates.")}
   return ["clicked":true,"clicked_label":clickedLabel]
  }else{
   let point: CGPoint
   if let x=args["screen_x"] as? Double, let y=args["screen_y"] as? Double, (0..<1).contains(x), (0..<1).contains(y) {
    let bounds=CGDisplayBounds(CGMainDisplayID())
    point=CGPoint(x:bounds.minX + x * bounds.width,y:bounds.minY + y * bounds.height)
   } else if let x=args["x"] as? Double,let y=args["y"] as? Double,x.isFinite,y.isFinite {
    point=CGPoint(x:x,y:y)
   } else { throw ToolError("Click needs an observed element or valid screen_x and screen_y fractions.") }
   guard let window=attr(root,kAXFocusedWindowAttribute) as! AXUIElement?,
     let position=attr(window,kAXPositionAttribute) as! AXValue?, let size=attr(window,kAXSizeAttribute) as! AXValue?
     else {throw ToolError("Could not verify the target window before clicking. Observe again.")}
   var origin=CGPoint.zero, dimensions=CGSize.zero
   guard AXValueGetValue(position,.cgPoint,&origin),AXValueGetValue(size,.cgSize,&dimensions),
     CGRect(origin:origin,size:dimensions).contains(point) else {throw ToolError("The click is outside the target app’s focused window. Observe again.")}
   CGEvent(mouseEventSource:nil,mouseType:.leftMouseDown,mouseCursorPosition:point,mouseButton:.left)?.post(tap:.cghidEventTap);CGEvent(mouseEventSource:nil,mouseType:.leftMouseUp,mouseCursorPosition:point,mouseButton:.left)?.post(tap:.cghidEventTap)
  };return ["clicked":true]
 }
 if action=="type"{
  guard let text=args["text"] as? String,text.utf16.count<=8000 else{throw ToolError("Text exceeds limit")}
  if let path=args["element"] as? String{let element=try node(root,path);guard AXUIElementSetAttributeValue(element,kAXFocusedAttribute as CFString,kCFBooleanTrue) == .success else{throw ToolError("Could not focus text field")}}
  let characters=Array(text.utf16);for down in [true,false]{let event=CGEvent(keyboardEventSource:nil,virtualKey:0,keyDown:down);characters.withUnsafeBufferPointer{event?.keyboardSetUnicodeString(stringLength:characters.count,unicodeString:$0.baseAddress)};event?.post(tap:.cghidEventTap)};return ["typed_characters":text.count]
 }
 if action=="key"{
  let keys:[String:CGKeyCode]=["return":36,"enter":36,"tab":48,"space":49,"escape":53,"delete":51,"backspace":51,"left":123,"right":124,"down":125,"up":126,"a":0,"c":8,"v":9,"x":7,"z":6,"l":37,"t":17,"w":13,"s":1,"f":3]
  let parts=(args["key"] as? String ?? "").lowercased().split(separator:"+").map(String.init);guard let last=parts.last,let key=keys[last]else{throw ToolError("Unsupported key. Use Return, Escape, arrows, or Cmd+A/C/V/X/Z/L/T/W/S/F.")}
  var flags=CGEventFlags();if parts.contains("cmd"){flags.insert(.maskCommand)};if parts.contains("shift"){flags.insert(.maskShift)};if parts.contains("alt"){flags.insert(.maskAlternate)};if parts.contains("ctrl"){flags.insert(.maskControl)}
  for down in [true,false]{let event=CGEvent(keyboardEventSource:nil,virtualKey:key,keyDown:down);event?.flags=flags;event?.post(tap:.cghidEventTap)};return ["key":parts.joined(separator:"+")]
 }
 if action=="scroll"{let amount=max(-1000,min(1000,args["pixels"] as? Int ?? -300));CGEvent(scrollWheelEvent2Source:nil,units:.pixel,wheelCount:1,wheel1:Int32(amount),wheel2:0,wheel3:0)?.post(tap:.cghidEventTap);return ["scrolled":amount]}
 throw ToolError("Unknown computer action")
}
do {
 let data = FileHandle.standardInput.readDataToEndOfFile()
 let args = try JSONSerialization.jsonObject(with: data) as? [String: Any] ?? [:]
 var result = try run(args)
 let action = args["action"] as? String ?? "observe"
 if result["window"] == nil,
    ["observe","open_app","click","type","key","scroll"].contains(action),
    let target = app(result["app"] as? String ?? args["app"] as? String) {
  attachWindow(&result, target: target)
 }
 emit(["result":result])
} catch {
 emit(["error":String(describing:error)])
 exit(1)
}
