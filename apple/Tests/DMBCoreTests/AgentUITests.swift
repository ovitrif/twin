import Testing
@testable import DMBCore

@Test func agentUIReadsTheSettingAsTOMLWritesIt() {
    #expect(AgentUI.allowed(config: "") == true)
    #expect(AgentUI.allowed(config: "window_height = 900\nagent_ui = true\n") == true)
    #expect(AgentUI.allowed(config: "agent_ui = false\n") == false)
    // A comment or a Windows line end used to read as on
    #expect(AgentUI.allowed(config: "agent_ui = false # off\n") == false)
    #expect(AgentUI.allowed(config: "agent_ui = false\r\nwindow_height = 900\r\n") == false)
    #expect(AgentUI.allowed(config: "agent_ui\t=\tfalse") == false)
    #expect(AgentUI.allowed(config: "# agent_ui = false\n") == true)
    #expect(AgentUI.allowed(config: "[other]\nagent_ui = false\n") == true)
}
