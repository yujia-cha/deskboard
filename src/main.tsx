import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// 데스크톱 위젯: 우클릭 메뉴 등 브라우저 기본 동작 차단
document.addEventListener("contextmenu", (e) => e.preventDefault());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
