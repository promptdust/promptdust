Feature: Browsing findings

  Findings group by tool; groups collapse by default except the one holding the
  selection; the attention filter narrows the list.

  Background:
    Given a machine with AI-tool data present
    And no previous scans

  Scenario: Groups are collapsed except the one holding the selection
    When I open PromptDust
    And I click "Scan"
    Then I see "history.jsonl"
    And I do not see "state.vscdb"

  Scenario: Expanding a collapsed group reveals its findings
    When I open PromptDust
    And I click "Scan"
    And I expand the "Cursor" group
    Then I see "state.vscdb"

  Scenario: Filtering by a level narrows the list and auto-expands matches
    When I open PromptDust
    And I click "Scan"
    And I filter by "high"
    Then I see "state.vscdb"
    And I do not see "history.jsonl"

  Scenario: Selecting a finding while a filter is active marks THAT finding read
    When I open PromptDust
    And I click "Scan"
    And I filter by "high"
    And I select the finding "state.vscdb"
    Then the finding "state.vscdb" is read

  Scenario: A tool can be collapsed while a filter is active
    When I open PromptDust
    And I click "Scan"
    And I filter by "high"
    Then I see "state.vscdb"
    When I click the "Cursor" tool header
    Then I do not see "state.vscdb"
