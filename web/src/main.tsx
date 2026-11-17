import { render } from "preact";
import { App } from "./app";
import "./styles.css";

document.documentElement.dataset.theme = "dark";
render(<App />, document.getElementById("app")!);
