# Changelog

- Pressing Option now creates writing that matches the app being used and what is visible on screen.
- Screen context is refreshed before every response, and on-screen text cannot secretly instruct the AI.
- Old screen context is cleared so it cannot affect a new response.
- Cleaned up the Option-key feature code to make it easier to maintain.
- Updated project documentation to show which Option-key improvements are complete.
- Rewriting only changes the text the user is currently working on.
- Users can choose how long Visual Recall data is kept and see an estimated storage cost for each option.
- Quickly pressing Option twice opens Scribe mode, while the notch only activates when the pointer is directly over it.
- Scribe now finishes reliably, returns the cursor to the right place, and closes correctly.
- Scribe now works when the selected text box is empty.
- Scribe keeps the user's original text visible while generating and clears completed requests correctly.
- Voice dictation adds new words at the cursor without selecting, deleting, or replacing existing text.
