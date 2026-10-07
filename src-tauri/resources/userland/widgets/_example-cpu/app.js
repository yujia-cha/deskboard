// HTML 위젯 예제. window.deskboard 로 대시보드와 이야기한다 (README.md 의 "HTML 위젯" 참고).
const $ = (id) => document.getElementById(id);
let warnAt = 80;
let peak = 0;

// 설정이 바뀌면(처음에도 한 번) 불린다
deskboard.onProps(({ settings }) => {
  $("label").textContent = settings.label || "CPU";
  warnAt = Number(settings.warnAt) || 80;
});

// 최고값은 이 위젯만의 저장소에 둔다 — 다시 시작해도 남는다
deskboard.ready.then(async () => {
  peak = (await deskboard.storage.get())?.peak ?? 0;
  $("peak").textContent = `최고 ${Math.round(peak)}%`;
});

// widget.json 의 "subscribe" 에 적은 이벤트만 받을 수 있다
deskboard.subscribe("sysmon://update", (s) => {
  const v = s.cpu_usage;
  $("value").textContent = `${Math.round(v)}%`;
  $("value").classList.toggle("hot", v >= warnAt);
  $("fill").style.width = `${Math.min(100, v)}%`;
  if (v > peak) {
    peak = v;
    $("peak").textContent = `최고 ${Math.round(peak)}%`;
    deskboard.storage.set({ peak });
  }
});

$("reset").addEventListener("click", () => {
  peak = 0;
  $("peak").textContent = "최고 —";
  deskboard.storage.set({ peak });
});
