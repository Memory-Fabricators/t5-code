import { env } from "./src/run";
console.log("[probe] before import setup");
setTimeout(() => {
	console.log("[probe] 5s timeout fired, import still pending");
	process.exit(3);
}, 5000);
import("./src/setup.js").then(() => {
	console.log("[probe] setup.js imported OK");
	process.exit(0);
});
