import { render } from "preact";
import { App } from "./app";
import "./styles.css";

if (import.meta.env.DEV && location.search.includes("mock")) await import("./dev-mock");
document.documentElement.dataset.theme = "dark";
render(<App />, document.getElementById("app")!);
