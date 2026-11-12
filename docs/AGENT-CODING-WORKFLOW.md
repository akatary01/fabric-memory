# Coding workflow

Read the applicable project instructions and inspect the current implementation, its callers, and the tests before choosing a change. Resolve the requested outcome into concrete checks, including an error case where relevant. Reuse established modules and patterns. Keep unrelated files and user changes intact.

Make the smallest coherent implementation, then run the checks closest to the behavior. Follow build errors to the relevant file and line before making another edit. For a user-visible change, exercise the normal entry point and check what actually happened. A tool call that merely started is not proof of completion.

Report the behavior delivered, the checks that ran, and any limitation that remains. Keep deployment, dependency upgrades, and commits within the requested scope.
