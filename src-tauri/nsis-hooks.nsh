; NSIS 설치/제거 훅 (tauri.conf.json 의 bundle.windows.nsis.installerHooks)
;
; **왜 필요한가.** deskboard 는 창이 작업표시줄에 없고 트레이에만 있는 상주 앱이다.
; 덮어 설치하면 NSIS 가 먼저 옛 버전의 언인스톨러를 돌리는데, 그때 앱이 돌고 있으면
; `deskboard.exe` 가 잠겨 있어 지우지 못하고 **"기존 버전을 제거할 수 없습니다"** 로 멈춘다.
; 자동 시작이 켜져 있으므로 재부팅한 PC 에서는 사실상 **항상** 돌고 있다.
;
; Tauri 의 기본 템플릿에도 "실행 중이면 닫아 달라"는 안내가 있지만, 사용자가 닫을 창이
; 눈에 보이지 않으니 그 안내로는 빠져나갈 수 없다. 그래서 여기서 직접 끝낸다.
;
; 앱 자신이 띄운 업데이트 설치(`installMode: passive`)에서도 이 훅이 돈다 — 우리 프로세스는
; 죽지만 설치 프로그램은 별개의 프로세스라 그대로 이어지고, 설치가 끝나면 다시 뜬다.
;
; **어느 쪽 훅이 실제로 구해 주는가.** 생성된 `installer.nsi` 를 보면 덮어 설치는
; 재설치 페이지에서 **옛 버전의 언인스톨러를 `ExecWait` 로 먼저 돌리고**, 그게 실패하면
; "$(unableToUninstall)" 을 띄우고 멈춘다. 그 시점은 새 설치본의 `NSIS_HOOK_PREINSTALL`
; **보다 앞**이다. 즉 이 파일이 실제로 문제를 없애는 것은 `NSIS_HOOK_PREUNINSTALL` 쪽이고,
; 그 효과는 **이 버전이 설치된 다음부터** 나타난다 (그때부터 옛 언인스톨러 = 이 훅을 가진
; 언인스톨러). 이 버전을 처음 덮어 설치할 때는 옛 언인스톨러에 훅이 없으므로, 앱이 떠 있으면
; 여전히 한 번은 트레이에서 종료해야 할 수 있다.
; PREINSTALL 쪽도 남겨 둔다 — 설치 도중 파일을 덮어쓰지 못하는 두 번째 실패를 막는다.

; 앱을 끝내고 파일 잠금이 풀릴 때까지 잠깐 기다린다.
; /T 는 자식 프로세스(webview 등)까지 함께 끝낸다. 안 돌고 있으면 그냥 실패하고 넘어간다.
!macro DESKBOARD_KILL
  nsExec::Exec 'taskkill /F /T /IM deskboard.exe'
  Pop $0
  ; 종료 직후에는 핸들이 아직 살아 있을 수 있다. 파일 삭제가 실패하지 않을 만큼만 쉰다.
  Sleep 1200
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro DESKBOARD_KILL
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro DESKBOARD_KILL
!macroend
