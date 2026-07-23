Feature: Scan history (the Inbox)

  Past runs persist locally so the Inbox can show history and per-item triage state.

  Scenario: History from previous runs is available and openable
    Given a machine with AI-tool data present
    And 2 previous scans
    When I open PromptDust
    Then I see the workspace
    When I open the Inbox
    Then I see 2 runs in the Inbox

  Scenario: A new scan is prepended to the Inbox
    Given a machine with AI-tool data present
    And 2 previous scans
    When I open PromptDust
    And I open the Inbox
    And I click "New scan"
    Then I see 3 runs in the Inbox

  Scenario: A pin survives a relaunch (history persists on disk)
    Given a machine with AI-tool data present
    And 2 previous scans
    When I open PromptDust
    And I expand the "Cursor" group
    And I select the finding "state.vscdb"
    And I open the finding menu
    And I click "Pin"
    Then the finding "state.vscdb" is pinned
    When I relaunch the app
    And I expand the "Cursor" group
    Then the finding "state.vscdb" is pinned
