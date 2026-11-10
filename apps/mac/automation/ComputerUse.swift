import AppKit
import ApplicationServices
import Foundation

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
func label(_ element:AXUIElement)->String{let role=attr(element,kAXRoleAttribute) as? String ?? "";let title=attr(element,kAXTitleAttribute) as? String ?? attr(element,kAXDescriptionAttribute) as? String ?? "";let value=(role=="AXSecureTextField" || (attr(element,kAXSubroleAttribute) as? String)=="AXSecureTextField") ? "[secure]":(attr(element,kAXValueAttribute) as? String ?? "");return [role,title,String(value.prefix(250))].filter{!$0.isEmpty}.joined(separator:" · ")}
func node(_ root:AXUIElement,_ path:String)throws->AXUIElement{var item=root;for part in path.split(separator:"."){guard let index=Int(part),children(item).indices.contains(index)else{throw ToolError("The UI changed. Observe the app again.")};item=children(item)[index]};return item}
func trusted()throws{guard AXIsProcessTrusted()else{throw ToolError("Enable Accessibility for Memory Fabric in System Settings before using Mac controls.")}}
func run(_ args:[String:Any])throws->[String:Any]{
 let action=args["action"] as? String ?? "observe"
 if action=="permissions"{return ["accessibility":AXIsProcessTrusted(),"screen_recording":CGPreflightScreenCaptureAccess()]}
 if action=="request_permissions"{
  let options=[kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String:true] as CFDictionary
  _=AXIsProcessTrustedWithOptions(options)
  if !CGPreflightScreenCaptureAccess(){_=CGRequestScreenCaptureAccess()}
  return ["accessibility":AXIsProcessTrusted(),"screen_recording":CGPreflightScreenCaptureAccess()]
 }
 if action=="resolve_app"{return try resolved(args["app"] as? String ?? "browser")}
 if action=="open_app"{
  let input=args["app"] as? String ?? "browser";let url=try applicationURL(input)
  guard NSWorkspace.shared.open(url)else{throw ToolError("Could not open that app")};var result=try resolved(input);result["opened"]=result["app"];return result
 }
 if action=="search_browser"{
  guard let query=args["query"] as? String,!query.trimmingCharacters(in:.whitespacesAndNewlines).isEmpty,query.utf8.count<=2000 else{throw ToolError("Browser search needs a query up to 2,000 bytes.")}
  var url=URLComponents(string:"https://www.google.com/search")!;url.queryItems=[URLQueryItem(name:"q",value:query)];return try run(["action":"open_url","url":url.url!.absoluteString])
 }
 if action=="open_url"{
  guard let raw=args["url"] as? String,raw.utf8.count<=8192,let url=URL(string:raw),["https","http"].contains(url.scheme?.lowercased() ?? ""),url.host != nil,url.user == nil,url.password == nil else{throw ToolError("Use a complete HTTP or HTTPS URL without embedded credentials.")}
  var result=try resolved("browser");guard NSWorkspace.shared.open(url)else{throw ToolError("The default browser could not open the URL.")};result["navigated_url"]=url.absoluteString;result["verification"]="Navigation dispatched; observe the browser to verify the loaded page.";return result
 }
 if action=="screenshot"{
  guard CGPreflightScreenCaptureAccess()else{throw ToolError("Enable Screen Recording for Memory Fabric in System Settings.")}
  guard let directory=ProcessInfo.processInfo.environment["FABRIC_DATA_DIR"]else{throw ToolError("Missing app storage")}
  let folder=URL(fileURLWithPath:directory).appendingPathComponent("Captures");try FileManager.default.createDirectory(at:folder,withIntermediateDirectories:true,attributes:[.posixPermissions:0o700]);let file=folder.appendingPathComponent(UUID().uuidString+".png")
  let process=Process();process.executableURL=URL(fileURLWithPath:"/usr/sbin/screencapture");process.arguments=["-x",file.path];try process.run();process.waitUntilExit();guard process.terminationStatus==0 else{throw ToolError("Screenshot failed")};try FileManager.default.setAttributes([.posixPermissions:0o600],ofItemAtPath:file.path)
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
   let element=try node(root,path)
   if let expected=args["expected"] as? String,!expected.isEmpty,!label(element).contains(expected){throw ToolError("The target changed. Observe again before clicking.")}
   guard AXUIElementPerformAction(element,kAXPressAction as CFString) == .success else{throw ToolError("This element cannot be pressed; use its screen coordinates.")}
  }else{
   guard let x=args["x"] as? Double,let y=args["y"] as? Double,x.isFinite,y.isFinite else{throw ToolError("Click needs an element path or x/y coordinates")}
   let point=CGPoint(x:x,y:y);CGEvent(mouseEventSource:nil,mouseType:.leftMouseDown,mouseCursorPosition:point,mouseButton:.left)?.post(tap:.cghidEventTap);CGEvent(mouseEventSource:nil,mouseType:.leftMouseUp,mouseCursorPosition:point,mouseButton:.left)?.post(tap:.cghidEventTap)
  };return ["clicked":true]
 }
 if action=="type"{
  guard let text=args["text"] as? String,text.utf16.count<=8000 else{throw ToolError("Text exceeds limit")}
  if let path=args["element"] as? String{let element=try node(root,path);guard AXUIElementSetAttributeValue(element,kAXFocusedAttribute as CFString,kCFBooleanTrue) == .success else{throw ToolError("Could not focus text field")}}
  let characters=Array(text.utf16);for down in [true,false]{let event=CGEvent(keyboardEventSource:nil,virtualKey:0,keyDown:down);characters.withUnsafeBufferPointer{event?.keyboardSetUnicodeString(stringLength:characters.count,unicodeString:$0.baseAddress)};event?.post(tap:.cghidEventTap)};return ["typed_characters":text.count]
 }
 if action=="key"{
  let keys:[String:CGKeyCode]=["return":36,"enter":36,"tab":48,"space":49,"escape":53,"delete":51,"backspace":51,"left":123,"right":124,"down":125,"up":126,"a":0,"c":8,"v":9,"x":7,"z":6,"l":37,"w":13,"s":1,"f":3]
  let parts=(args["key"] as? String ?? "").lowercased().split(separator:"+").map(String.init);guard let last=parts.last,let key=keys[last]else{throw ToolError("Unsupported key. Use Return, Escape, arrows, or Cmd+A/C/V/X/Z/L/W/S/F.")}
  var flags=CGEventFlags();if parts.contains("cmd"){flags.insert(.maskCommand)};if parts.contains("shift"){flags.insert(.maskShift)};if parts.contains("alt"){flags.insert(.maskAlternate)};if parts.contains("ctrl"){flags.insert(.maskControl)}
  for down in [true,false]{let event=CGEvent(keyboardEventSource:nil,virtualKey:key,keyDown:down);event?.flags=flags;event?.post(tap:.cghidEventTap)};return ["key":parts.joined(separator:"+")]
 }
 if action=="scroll"{let amount=max(-1000,min(1000,args["pixels"] as? Int ?? -300));CGEvent(scrollWheelEvent2Source:nil,units:.pixel,wheelCount:1,wheel1:Int32(amount),wheel2:0,wheel3:0)?.post(tap:.cghidEventTap);return ["scrolled":amount]}
 throw ToolError("Unknown computer action")
}
do{let data=FileHandle.standardInput.readDataToEndOfFile();let args=try JSONSerialization.jsonObject(with:data) as? [String:Any] ?? [:];emit(["result":try run(args)])}catch{emit(["error":String(describing:error)]);exit(1)}
