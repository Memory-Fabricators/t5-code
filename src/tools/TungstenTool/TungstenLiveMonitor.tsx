import React from "react";

/**
 * Placeholder for the optional Tungsten live monitor.  Tungsten is not part of
 * the source distribution, but REPL imports this component unconditionally;
 * keeping a no-op component here lets the normal Ink UI load without making
 * the optional feature a runtime dependency.
 */
export function TungstenLiveMonitor(): React.ReactNode {
	return null;
}
