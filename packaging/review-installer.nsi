; Draft per-user review installer route. No compiler invoked; no app autorun.
; Compile only after native acceptance and exact payload/runtime binding review.
Unicode true
RequestExecutionLevel user
Name "Lifeinvader Native Review"
OutFile "Lifeinvader-Native-Review-Setup.exe"
InstallDir "$LOCALAPPDATA\LifeinvaderReview\versions\a7428a460"
Page directory
Page instfiles
Section
  SetOverwrite off
  SetOutPath "$INSTDIR"
  File /r "..\tools\first-review-package-02\payload\*"
  ; No DB, queue, media, credentials, environment settings or startup tasks.
  ; No Exec/ExecShell: manual launch needs its separately reviewed plan.
SectionEnd
