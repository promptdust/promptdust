Feature: The finding detail pane

  The persistent detail pane explains a finding and offers only read-only actions:
  reveal the folder and share a metadata summary. There is never a delete affordance.

  Background:
    Given a machine with AI-tool data present
    And no previous scans

  Scenario: The selected finding shows why it matters, its amplifiers, and guidance
    When I open PromptDust
    And I click "Scan"
    Then I see "Verbatim transcripts"
    And I see "Cloud-synced"
    And I see "Reveal in Finder"
    And I do not see the word "delete"

  Scenario: Reveal opens the folder (read-only)
    When I open PromptDust
    And I click "Scan"
    And I click "Reveal in Finder"
    Then the reveal command ran

  Scenario: Share hands a metadata summary to the native sheet
    When I open PromptDust
    And I click "Scan"
    And I open the share menu
    And I click "Share…"
    Then the share command ran

  Scenario: Share falls back to the clipboard when the sheet is unavailable
    Given the native share sheet is unavailable
    When I open PromptDust
    And I click "Scan"
    And I open the share menu
    And I click "Share…"
    Then a toast says "Summary copied"
