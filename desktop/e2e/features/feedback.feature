Feature: Feedback and privacy

  Anonymous sharing is opt-in and off by default; everything the app could ever send is
  inspectable before it leaves the machine.

  Background:
    Given a machine with AI-tool data present
    And no previous scans

  Scenario: Sharing is off by default and turns on through consent
    When I open PromptDust
    And I click "Anonymous sharing"
    Then I see "Help improve PromptDust?"
    And I see "off by default"
    When I click "Turn on sharing"
    Then a toast says "Anonymous sharing on"

  Scenario: The telemetry preview shows exactly what would be sent
    When I open PromptDust
    And I click "Scan"
    And I open Settings
    And I click "Preview what's sent"
    Then I see "promptdust-telemetry"

  Scenario: The diagnostics bundle can be inspected before sharing
    When I open PromptDust
    And I click "Scan"
    And I open Settings
    And I click "Create…"
    Then I see "promptdust-diagnostics"

  Scenario: When the environment forces sharing off, there is no live toggle
    Given anonymous sharing is forced off by the environment
    When I open PromptDust
    And I click "Scan"
    And I open Settings
    Then I see "Forced off by the environment"

  Scenario: Check for updates reports status in Settings (no full re-download)
    When I open PromptDust
    And I click "Scan"
    And I open Settings
    And I click "Check"
    Then I see "You're on the latest version"
