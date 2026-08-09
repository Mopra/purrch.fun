; Uninstall cleanup.
;
; The NSIS uninstaller removes the program files and nothing else, so without
; this a "full" uninstall leaves the colony behind: cats.json, chores.json and
; auth.json sit in %APPDATA% forever, and reinstalling brings back cats the user
; thought they had said goodbye to.
;
; Only ever the app's own identifier directory — never %APPDATA% itself, and
; never a path assembled from anything the user typed.
;
; What this deliberately does NOT remove is the saved API key: it lives in
; Windows Credential Manager under `fun.purrch.keys`, and deleting credentials
; from an uninstaller is the kind of thing that goes wrong quietly and takes
; something else with it. The panel's "go back to your subscription" button
; forgets it properly, and README.md says so.

; Both profiles, because Purrch uses both. The colony's own files are roaming —
; they are small and worth following you to another machine — while the speech
; engine and its model are a ~150 MB download that is emphatically not, and so
; live in the local profile. Removing only the first would leave the larger half
; behind, which is the half a user would notice.

!macro NSIS_HOOK_POSTUNINSTALL
  ; $AppData / $LocalAppData are the profiles of the user running the
  ; uninstaller, which is the user whose cats these were.
  RMDir /r "$AppData\fun.purrch.pet"
  RMDir /r "$LocalAppData\fun.purrch.pet"
!macroend
