Feature: Running a scan

  The welcome screen leads to a scan; the result opens the workspace, or an honest
  empty / permission state. Nothing ever reads as a pass/fail verdict.

  Scenario: First scan opens the workspace with two honest numbers
    Given a machine with AI-tool data present
    And no previous scans
    When I open PromptDust
    And I click "Scan"
    Then I see the workspace
    And the Exposure score is shown
    And the Confidence score is shown

  Scenario: An empty machine never reads as "clean"
    Given a machine with no AI-tool data
    And no previous scans
    When I open PromptDust
    And I click "Scan"
    Then I see "No data from known AI tools found"
    And I do not see the word "clean"
    And I do not see the word "safe"

  Scenario: A failed scan surfaces the permission screen, read-only
    Given the scan fails with "Library access denied"
    And no previous scans
    When I open PromptDust
    And I click "Scan"
    Then I see "Permission needed"
    And I see "Library access denied"
