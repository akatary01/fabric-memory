# Screen and audio recording

Record only when the user requests recording. Confirm which screen content, audio source, and duration the task calls for. The recording action saves a main-display movie to the enabled workspace. It accepts 1 to 60 seconds and audio set to none, microphone, or system. When the audio source is ambiguous, ask which one to capture.

Check macOS permissions when recording fails. A successful receipt includes the saved path and verified tracks. Report the path and the audio source that was captured. Never upload a recording as part of capture.

## Worked example

Request: “Record the main display for ten seconds with microphone audio.” Confirm recording and workspace permissions. Use the record action with duration 10 and microphone audio. After it returns, check the receipt for the movie path and the requested audio track. Report the saved path, duration requested, and audio source. A screenshot, an unverified file, or a launch receipt is not a recording.

If the user says “record this” and two displays or an audio source matter, ask which content and audio they want. Do not silently enable the microphone or system audio. If recording fails, give the exact permission or missing-track error. Do not claim that a partially created file is usable.

## Interpret the options

Use `computer(action="record", seconds=10, audio="microphone")` for the example above. Use `audio="system"` when the user requests the audio playing from apps, and `audio="none"` for a silent recording. The workspace path is the saved deliverable. For a request longer than the tool permits, explain the limit and ask what interval matters. Do not silently shorten it or start multiple recordings.

Check that a recording error names the actual failing stage. A denied permission, insufficient task time, or missing track needs a different fix. Avoid retrying the identical action until the condition changes.
