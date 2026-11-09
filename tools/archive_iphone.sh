#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
: "${FABRIC_TEAM_ID:?Set FABRIC_TEAM_ID to your Apple Developer team}"
test -f apps/iphone/DemoAssets/demo.json || { echo 'Run tools/stage_testflight.py first.' >&2; exit 1; }
/usr/bin/python3 tools/stage_phone_generation.py
python3 tools/generate_xcode.py
archive_path="${FABRIC_ARCHIVE_PATH:-$PWD/../MemoryFabric.xcarchive}"
xcodebuild -project apps/iphone/FabricPhone.xcodeproj -scheme FabricRunner \
  -configuration Release -destination 'generic/platform=iOS' \
  -derivedDataPath .build/iphone -archivePath "$archive_path" \
  -allowProvisioningUpdates DEVELOPMENT_TEAM="$FABRIC_TEAM_ID" archive
python3 tools/package_archive.py "$archive_path" "$(dirname "$archive_path")/MemoryFabric-TestFlight.zip"
echo "Archive prepared: $archive_path"
echo 'Open it in Xcode Organizer and choose Distribute App → App Store Connect.'
