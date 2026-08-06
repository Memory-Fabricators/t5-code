import { formatTotalCost } from "../../cost-tracker.js";
import { currentLimits } from "../../services/claudeAiLimits.js";
import type { LocalCommandCall } from "../../types/command.js";
import { isClaudeAISubscriber } from "../../utils/auth.js";

export const call: LocalCommandCall = async () => {
	return { type: "text", value: formatTotalCost() };
};
