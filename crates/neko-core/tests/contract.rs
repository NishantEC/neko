//! The agent contract, run against every runtime: the in-process fake, and Claude Code driven by the fake CLI.

mod support;
use support::*;

#[test]
fn the_fake_runtime_keeps_the_contract() {
    contract_suite(&FakeRuntime);
}

#[test]
fn claude_code_on_the_fake_cli_keeps_the_contract() {
    contract_suite(&claude_on_fake());
}
