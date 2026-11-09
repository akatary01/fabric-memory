#!/usr/bin/env python3
"""Generate the intentionally UI-free physical-iPhone host project; no xcodegen dependency."""
from pathlib import Path
import re
ROOT=Path(__file__).resolve().parents[1]
project=ROOT/"apps/iphone/FabricPhone.xcodeproj"
existing=(project/"project.pbxproj").read_text() if (project/"project.pbxproj").exists() else ""
def existing_value(key):
    match=re.search(r"\b"+key+r"\s*=\s*([^;]+);",existing)
    return match.group(1).strip() if match else None
bundle_id=existing_value("PRODUCT_BUNDLE_IDENTIFIER") or "local.memoryfabric.phone"
team=existing_value("DEVELOPMENT_TEAM")
project.mkdir(exist_ok=True)
def ident(n): return f"{n:024X}"
ids={name:ident(i+1) for i,name in enumerate(["project","group","products","app","target","sources","frameworks","resources","source","sourcebuild","package","product","productbuild","projconfigs","projdebug","projrelease","targetconfigs","targetdebug","targetrelease","assets","assetsbuild","icons","iconsbuild","privacy","privacybuild","chat","chatbuild"])}
def r(name): return ids[name]
objects=[]
def add(name,content): objects.append(f"{r(name)} = {{ {content} }};")
add("project",f"isa = PBXProject; buildConfigurationList = {r('projconfigs')}; compatibilityVersion = \"Xcode 14.0\"; developmentRegion = en; knownRegions = (en,Base); mainGroup = {r('group')}; productRefGroup = {r('products')}; projectDirPath = \"\"; projectRoot = \"\"; packageReferences = ({r('package')}); targets = ({r('target')});")
add("group",f"isa = PBXGroup; children = ({r('source')},{r('chat')},{r('assets')},{r('icons')},{r('privacy')},{r('products')}); sourceTree = \"<group>\";")
add("products",f"isa = PBXGroup; children = ({r('app')}); name = Products; sourceTree = \"<group>\";")
add("app",'isa = PBXFileReference; explicitFileType = wrapper.application; path = FabricRunner.app; sourceTree = BUILT_PRODUCTS_DIR;')
add("source",'isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = Runner/AppDelegate.swift; sourceTree = "<group>";')
add("sourcebuild",f"isa = PBXBuildFile; fileRef = {r('source')};")
add("chat",'isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = Runner/ChatController.swift; sourceTree = "<group>";')
add("chatbuild",f"isa = PBXBuildFile; fileRef = {r('chat')};")
add("assets",'isa = PBXFileReference; lastKnownFileType = folder; path = DemoAssets; sourceTree = "<group>";')
add("assetsbuild",f"isa = PBXBuildFile; fileRef = {r('assets')};")
add("icons",'isa = PBXFileReference; lastKnownFileType = folder.assetcatalog; path = Runner/Assets.xcassets; sourceTree = "<group>";')
add("iconsbuild",f"isa = PBXBuildFile; fileRef = {r('icons')};")
add("privacy",'isa = PBXFileReference; lastKnownFileType = text.xml; path = Runner/PrivacyInfo.xcprivacy; sourceTree = "<group>";')
add("privacybuild",f"isa = PBXBuildFile; fileRef = {r('privacy')};")
add("productbuild",f"isa = PBXBuildFile; productRef = {r('product')};")
add("sources",f"isa = PBXSourcesBuildPhase; buildActionMask = 2147483647; files = ({r('sourcebuild')},{r('chatbuild')}); runOnlyForDeploymentPostprocessing = 0;")
add("frameworks",f"isa = PBXFrameworksBuildPhase; buildActionMask = 2147483647; files = ({r('productbuild')}); runOnlyForDeploymentPostprocessing = 0;")
add("resources",f"isa = PBXResourcesBuildPhase; buildActionMask = 2147483647; files = ({r('assetsbuild')},{r('iconsbuild')},{r('privacybuild')}); runOnlyForDeploymentPostprocessing = 0;")
add("package",'isa = XCLocalSwiftPackageReference; relativePath = ".";')
add("product",f"isa = XCSwiftPackageProductDependency; package = {r('package')}; productName = FabricPhoneCore;")
add("target",f"isa = PBXNativeTarget; buildConfigurationList = {r('targetconfigs')}; buildPhases = ({r('sources')},{r('frameworks')},{r('resources')}); buildRules = (); dependencies = (); name = FabricRunner; packageProductDependencies = ({r('product')}); productName = FabricRunner; productReference = {r('app')}; productType = \"com.apple.product-type.application\";")
for prefix in ["proj","target"]:
    add(prefix+"configs",f"isa = XCConfigurationList; buildConfigurations = ({r(prefix+'debug')},{r(prefix+'release')}); defaultConfigurationIsVisible = 0; defaultConfigurationName = Debug;")
    for mode in ["debug","release"]:
        if prefix == "proj":
            settings='CLANG_ENABLE_MODULES = YES; IPHONEOS_DEPLOYMENT_TARGET = 17.0; SDKROOT = iphoneos; SWIFT_VERSION = 5.0;'
        else:
            settings='PRODUCT_BUNDLE_IDENTIFIER = local.memoryfabric.phone; PRODUCT_NAME = FabricRunner; INFOPLIST_FILE = Runner/Info.plist; GENERATE_INFOPLIST_FILE = NO; CODE_SIGN_STYLE = Automatic; TARGETED_DEVICE_FAMILY = 1; SUPPORTED_PLATFORMS = iphoneos; SWIFT_VERSION = 5.0; IPHONEOS_DEPLOYMENT_TARGET = 17.0; LD_RUNPATH_SEARCH_PATHS = "$(inherited) @executable_path/Frameworks"; ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon;'
            settings=settings.replace("PRODUCT_BUNDLE_IDENTIFIER = local.memoryfabric.phone;","PRODUCT_BUNDLE_IDENTIFIER = "+bundle_id+";")
            if team:settings+=" DEVELOPMENT_TEAM = "+team+";"
        if mode == "release": settings += ' SWIFT_OPTIMIZATION_LEVEL = "-O"; DEBUG_INFORMATION_FORMAT = "dwarf-with-dsym";'
        else: settings += ' SWIFT_OPTIMIZATION_LEVEL = "-Onone";'
        add(prefix+mode,f'isa = XCBuildConfiguration; buildSettings = {{ {settings} }}; name = {mode.title()};')
(project/"project.pbxproj").write_text('// !$*UTF8*$!\n{ archiveVersion = 1; classes = {}; objectVersion = 56; objects = {\n'+"\n".join(objects)+f"\n}}; rootObject = {r('project')}; }}\n")
print(project)
