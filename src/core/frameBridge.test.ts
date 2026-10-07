import { describe, expect, it } from "vitest";
import { NS, VERSION, allowedEvent, dbwUrl, parseFrameMessage } from "./frameBridge";

const msg = (m: Record<string, unknown>) => ({ ns: NS, v: VERSION, ...m });

describe("parseFrameMessage — iframe 위젯이 할 수 있는 일의 경계", () => {
  it("모양이 다른 메시지는 버린다", () => {
    expect(parseFrameMessage(null, [])).toBeNull();
    expect(parseFrameMessage({ type: "ready" }, [])).toBeNull(); // ns 없음
    expect(parseFrameMessage(msg({ v: 99, type: "ready" }), [])).toBeNull();
    expect(parseFrameMessage(msg({ type: "nope" }), [])).toBeNull();
  });

  it("허용 목록 밖의 메서드는 거절", () => {
    expect(parseFrameMessage(msg({ type: "call", id: 1, method: "invoke", args: ["folder_recycle"] }), [])).toBeNull();
    expect(parseFrameMessage(msg({ type: "call", id: 1, method: "storage.get", args: [] }), [])?.type).toBe("call");
  });

  it("이벤트 구독은 widget.json 선언 ∩ 공개 목록", () => {
    const sub = (name: string, declared: string[]) =>
      parseFrameMessage(msg({ type: "call", id: 1, method: "events.subscribe", args: [name] }), declared);
    expect(sub("sysmon://update", ["sysmon://update"])).not.toBeNull();
    expect(sub("sysmon://update", [])).toBeNull(); // 선언 안 함
    expect(sub("spotify://playback", ["spotify://playback"])).toBeNull(); // 공개 목록 밖
    expect(allowedEvent("hit://outside-press", ["hit://outside-press"])).toBe(false);
  });

  it("openUrl 은 http/https 만", () => {
    const open = (url: unknown) => parseFrameMessage(msg({ type: "call", id: 1, method: "openUrl", args: [url] }), []);
    expect(open("https://example.com")).not.toBeNull();
    expect(open("file:///C:/Windows")).toBeNull();
    expect(open("javascript:alert(1)")).toBeNull();
    expect(open(42)).toBeNull();
  });

  it("settings.update 는 객체만", () => {
    const upd = (p: unknown) => parseFrameMessage(msg({ type: "call", id: 1, method: "settings.update", args: [p] }), []);
    expect(upd({ a: 1 })).not.toBeNull();
    expect(upd([1])).toBeNull();
    expect(upd("x")).toBeNull();
  });

  it("오류 메시지는 길이를 자른다", () => {
    const m = parseFrameMessage(msg({ type: "error", message: "x".repeat(5000) }), []);
    expect(m?.type === "error" && m.message.length).toBe(2000);
  });
});

describe("dbwUrl", () => {
  it("경로 구분자는 남기고 각 조각만 인코딩한다", () => {
    expect(dbwUrl("my-w", "./img/a b.png")).toMatch(/\/my-w\/img\/a%20b\.png$/);
  });
});
