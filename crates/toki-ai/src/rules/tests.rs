use super::*;

// ============================================================================
// Rule struct tests
// ============================================================================

#[test]
fn test_rule_clone() {
    let rule = Rule {
        pattern: "test".to_string(),
        category: "Testing".to_string(),
        confidence: 0.9,
    };
    let cloned = rule.clone();
    assert_eq!(cloned.pattern, "test");
    assert_eq!(cloned.category, "Testing");
    assert!((cloned.confidence - 0.9).abs() < f32::EPSILON);
}

#[test]
fn test_rule_debug() {
    let rule = Rule {
        pattern: "debug".to_string(),
        category: "Debug".to_string(),
        confidence: 0.5,
    };
    let debug = format!("{:?}", rule);
    assert!(debug.contains("Rule"));
    assert!(debug.contains("debug"));
    assert!(debug.contains("Debug"));
}

#[test]
fn test_rule_serialize() {
    let rule = Rule {
        pattern: "(?i)test".to_string(),
        category: "Testing".to_string(),
        confidence: 0.85,
    };
    let json = serde_json::to_string(&rule).unwrap();
    assert!(json.contains("test"));
    assert!(json.contains("Testing"));
    assert!(json.contains("0.85"));
}

#[test]
fn test_rule_deserialize() {
    let json = r#"{"pattern":"(?i)app","category":"Application","confidence":0.75}"#;
    let rule: Rule = serde_json::from_str(json).unwrap();
    assert_eq!(rule.pattern, "(?i)app");
    assert_eq!(rule.category, "Application");
    assert!((rule.confidence - 0.75).abs() < f32::EPSILON);
}

#[test]
fn test_rule_roundtrip() {
    let original = Rule {
        pattern: "complex.*pattern".to_string(),
        category: "Complex".to_string(),
        confidence: 0.99,
    };
    let json = serde_json::to_string(&original).unwrap();
    let restored: Rule = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.pattern, original.pattern);
    assert_eq!(restored.category, original.category);
    assert!((restored.confidence - original.confidence).abs() < f32::EPSILON);
}

// ============================================================================
// RuleEngine::new tests
// ============================================================================

#[test]
fn test_rule_engine_new() {
    let engine = RuleEngine::new();
    // New engine should have no rules
    assert!(engine.classify("anything").is_none());
}

#[test]
fn test_rule_engine_default() {
    let engine = RuleEngine::default();
    // Default is same as new - no rules
    assert!(engine.classify("anything").is_none());
}

// ============================================================================
// RuleEngine::add_rule tests
// ============================================================================

#[test]
fn test_add_rule_simple() {
    let mut engine = RuleEngine::new();
    let rule = Rule {
        pattern: "test".to_string(),
        category: "Testing".to_string(),
        confidence: 0.9,
    };
    let result = engine.add_rule(rule);
    assert!(result.is_ok());
}

#[test]
fn test_add_rule_regex() {
    let mut engine = RuleEngine::new();
    let rule = Rule {
        pattern: "(?i)(vscode|cursor)".to_string(),
        category: "Coding".to_string(),
        confidence: 0.95,
    };
    let result = engine.add_rule(rule);
    assert!(result.is_ok());
}

#[test]
fn test_add_rule_invalid_regex() {
    let mut engine = RuleEngine::new();
    let rule = Rule {
        pattern: "[invalid".to_string(), // Unclosed bracket
        category: "Invalid".to_string(),
        confidence: 0.5,
    };
    let result = engine.add_rule(rule);
    assert!(result.is_err());
}

#[test]
fn test_add_multiple_rules() {
    let mut engine = RuleEngine::new();

    let rule1 = Rule {
        pattern: "rule1".to_string(),
        category: "Cat1".to_string(),
        confidence: 0.9,
    };
    let rule2 = Rule {
        pattern: "rule2".to_string(),
        category: "Cat2".to_string(),
        confidence: 0.8,
    };

    assert!(engine.add_rule(rule1).is_ok());
    assert!(engine.add_rule(rule2).is_ok());

    // Both rules should work
    assert!(engine.classify("rule1").is_some());
    assert!(engine.classify("rule2").is_some());
}

// ============================================================================
// RuleEngine::load_default_rules tests
// ============================================================================

#[test]
fn test_load_default_rules() {
    let mut engine = RuleEngine::new();
    let result = engine.load_default_rules();
    assert!(result.is_ok());
}

#[test]
fn test_load_default_rules_classifies_vscode() {
    let mut engine = RuleEngine::new();
    engine.load_default_rules().unwrap();

    let result = engine.classify("com.microsoft.vscode");
    assert!(result.is_some());
    let (category, confidence) = result.unwrap();
    assert_eq!(category, "Coding");
    assert!((confidence - 0.95).abs() < f32::EPSILON);
}

#[test]
fn test_load_default_rules_classifies_cursor() {
    let mut engine = RuleEngine::new();
    engine.load_default_rules().unwrap();

    let result = engine.classify("com.todesktop.cursor");
    assert!(result.is_some());
    let (category, _) = result.unwrap();
    assert_eq!(category, "Coding");
}

#[test]
fn test_load_default_rules_classifies_intellij() {
    let mut engine = RuleEngine::new();
    engine.load_default_rules().unwrap();

    let result = engine.classify("com.jetbrains.intellij");
    assert!(result.is_some());
    let (category, _) = result.unwrap();
    assert_eq!(category, "Coding");
}

#[test]
fn test_load_default_rules_classifies_pycharm() {
    let mut engine = RuleEngine::new();
    engine.load_default_rules().unwrap();

    let result = engine.classify("com.jetbrains.pycharm");
    assert!(result.is_some());
    let (category, _) = result.unwrap();
    assert_eq!(category, "Coding");
}

#[test]
fn test_load_default_rules_classifies_terminal() {
    let mut engine = RuleEngine::new();
    engine.load_default_rules().unwrap();

    let result = engine.classify("com.apple.Terminal");
    assert!(result.is_some());
    let (category, confidence) = result.unwrap();
    assert_eq!(category, "Terminal");
    assert!((confidence - 0.90).abs() < f32::EPSILON);
}

#[test]
fn test_load_default_rules_classifies_iterm() {
    let mut engine = RuleEngine::new();
    engine.load_default_rules().unwrap();

    let result = engine.classify("com.googlecode.iterm2");
    assert!(result.is_some());
    let (category, _) = result.unwrap();
    assert_eq!(category, "Terminal");
}

#[test]
fn test_load_default_rules_classifies_chrome() {
    let mut engine = RuleEngine::new();
    engine.load_default_rules().unwrap();

    let result = engine.classify("com.google.Chrome");
    assert!(result.is_some());
    let (category, confidence) = result.unwrap();
    assert_eq!(category, "Browser");
    assert!((confidence - 0.85).abs() < f32::EPSILON);
}

#[test]
fn test_load_default_rules_classifies_firefox() {
    let mut engine = RuleEngine::new();
    engine.load_default_rules().unwrap();

    let result = engine.classify("org.mozilla.firefox");
    assert!(result.is_some());
    let (category, _) = result.unwrap();
    assert_eq!(category, "Browser");
}

#[test]
fn test_load_default_rules_classifies_safari() {
    let mut engine = RuleEngine::new();
    engine.load_default_rules().unwrap();

    let result = engine.classify("com.apple.Safari");
    assert!(result.is_some());
    let (category, _) = result.unwrap();
    assert_eq!(category, "Browser");
}

#[test]
fn test_load_default_rules_case_insensitive() {
    let mut engine = RuleEngine::new();
    engine.load_default_rules().unwrap();

    // Test lowercase
    assert!(engine.classify("vscode").is_some());
    // Test uppercase
    assert!(engine.classify("VSCODE").is_some());
    // Test mixed case
    assert!(engine.classify("VsCode").is_some());
}

// ============================================================================
// RuleEngine::classify tests
// ============================================================================

#[test]
fn test_classify_no_rules() {
    let engine = RuleEngine::new();
    let result = engine.classify("anything");
    assert!(result.is_none());
}

#[test]
fn test_classify_no_match() {
    let mut engine = RuleEngine::new();
    engine.add_rule(Rule {
        pattern: "specific".to_string(),
        category: "Specific".to_string(),
        confidence: 0.9,
    }).unwrap();

    let result = engine.classify("something_else");
    assert!(result.is_none());
}

#[test]
fn test_classify_exact_match() {
    let mut engine = RuleEngine::new();
    engine.add_rule(Rule {
        pattern: "myapp".to_string(),
        category: "MyCategory".to_string(),
        confidence: 0.88,
    }).unwrap();

    let result = engine.classify("myapp");
    assert!(result.is_some());
    let (category, confidence) = result.unwrap();
    assert_eq!(category, "MyCategory");
    assert!((confidence - 0.88).abs() < f32::EPSILON);
}

#[test]
fn test_classify_partial_match() {
    let mut engine = RuleEngine::new();
    engine.add_rule(Rule {
        pattern: "app".to_string(),
        category: "Application".to_string(),
        confidence: 0.7,
    }).unwrap();

    // "app" pattern should match "myapp"
    let result = engine.classify("myapp");
    assert!(result.is_some());
}

#[test]
fn test_classify_first_match_wins() {
    let mut engine = RuleEngine::new();
    engine.add_rule(Rule {
        pattern: "test".to_string(),
        category: "First".to_string(),
        confidence: 0.9,
    }).unwrap();
    engine.add_rule(Rule {
        pattern: "test".to_string(),
        category: "Second".to_string(),
        confidence: 0.8,
    }).unwrap();

    let result = engine.classify("test");
    assert!(result.is_some());
    let (category, _) = result.unwrap();
    assert_eq!(category, "First"); // First rule wins
}

#[test]
fn test_classify_regex_alternation() {
    let mut engine = RuleEngine::new();
    engine.add_rule(Rule {
        pattern: "(foo|bar|baz)".to_string(),
        category: "FooBarBaz".to_string(),
        confidence: 0.95,
    }).unwrap();

    assert!(engine.classify("foo").is_some());
    assert!(engine.classify("bar").is_some());
    assert!(engine.classify("baz").is_some());
    assert!(engine.classify("qux").is_none());
}

#[test]
fn test_classify_regex_wildcard() {
    let mut engine = RuleEngine::new();
    engine.add_rule(Rule {
        pattern: "app.*helper".to_string(),
        category: "Helper".to_string(),
        confidence: 0.8,
    }).unwrap();

    assert!(engine.classify("app.system.helper").is_some());
    assert!(engine.classify("apphelper").is_some());
    assert!(engine.classify("helper").is_none());
}

#[test]
fn test_classify_empty_app_id() {
    let mut engine = RuleEngine::new();
    engine.add_rule(Rule {
        pattern: ".*".to_string(), // Match anything
        category: "Any".to_string(),
        confidence: 0.5,
    }).unwrap();

    // Empty string still matches .*
    let result = engine.classify("");
    assert!(result.is_some());
}

#[test]
fn test_classify_unicode() {
    let mut engine = RuleEngine::new();
    engine.add_rule(Rule {
        pattern: "日本語".to_string(),
        category: "Japanese".to_string(),
        confidence: 0.9,
    }).unwrap();

    let result = engine.classify("日本語アプリ");
    assert!(result.is_some());
    let (category, _) = result.unwrap();
    assert_eq!(category, "Japanese");
}

// ============================================================================
// Edge cases and integration tests
// ============================================================================

#[test]
fn test_rule_with_special_regex_chars() {
    let mut engine = RuleEngine::new();
    // Escape special regex characters
    engine.add_rule(Rule {
        pattern: r"file\.txt".to_string(),
        category: "TextFile".to_string(),
        confidence: 0.9,
    }).unwrap();

    assert!(engine.classify("file.txt").is_some());
    assert!(engine.classify("filextxt").is_none());
}

#[test]
fn test_rule_anchor_start() {
    let mut engine = RuleEngine::new();
    engine.add_rule(Rule {
        pattern: "^com\\.".to_string(),
        category: "ComDomain".to_string(),
        confidence: 0.8,
    }).unwrap();

    assert!(engine.classify("com.example.app").is_some());
    assert!(engine.classify("org.com.example").is_none());
}

#[test]
fn test_rule_anchor_end() {
    let mut engine = RuleEngine::new();
    engine.add_rule(Rule {
        pattern: "\\.app$".to_string(),
        category: "MacApp".to_string(),
        confidence: 0.85,
    }).unwrap();

    assert!(engine.classify("MyProgram.app").is_some());
    assert!(engine.classify("MyProgram.app.backup").is_none());
}

#[test]
fn test_multiple_classifications() {
    let mut engine = RuleEngine::new();
    engine.load_default_rules().unwrap();

    // Test multiple different apps
    let apps = vec![
        ("vscode", "Coding"),
        ("terminal", "Terminal"),
        ("chrome", "Browser"),
    ];

    for (app, expected_category) in apps {
        let result = engine.classify(app);
        assert!(result.is_some(), "Failed to classify: {}", app);
        let (category, _) = result.unwrap();
        assert_eq!(category, expected_category, "Wrong category for: {}", app);
    }
}

#[test]
fn test_confidence_values() {
    let mut engine = RuleEngine::new();
    engine.load_default_rules().unwrap();

    // Coding should have highest confidence
    let (_, coding_conf) = engine.classify("vscode").unwrap();
    // Terminal should have medium confidence
    let (_, terminal_conf) = engine.classify("terminal").unwrap();
    // Browser should have lowest confidence among defaults
    let (_, browser_conf) = engine.classify("chrome").unwrap();

    assert!(coding_conf > terminal_conf);
    assert!(terminal_conf > browser_conf);
}
