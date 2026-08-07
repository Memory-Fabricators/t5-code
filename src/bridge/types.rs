//! Port of `src/bridge/types.ts` (partial stub).

pub const BRIDGE_LOGIN_INSTRUCTION: &str = "Remote Control is only available with claude.ai subscriptions. Please use `/login` to sign in with your claude.ai account.";

/// Full error printed when `claude remote-control` is run without auth.
pub const BRIDGE_LOGIN_ERROR: &str = concat!(
    "Error: You must be logged in to use Remote Control.\n\n",
    "Remote Control is only available with claude.ai subscriptions. Please use `/login` to sign in with your claude.ai account."
);
