        FAIL [   2.283s] codanna indexing::simple::tests::test_import_based_relationship_resolution
  stdout 

    running 1 test
    test indexing::simple::tests::test_import_based_relationship_resolution ... FAILED

    failures:

    failures:
        indexing::simple::tests::test_import_based_relationship_resolution

    test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 558 filtered out; finished in 1.62s

  stderr 
    DEBUG: No symbol cache found at C:\Users\admin\AppData\Local\Temp\.tmpiwaOME\.test_import_resolution\symbol_cache.bin
    DEBUG: reindex_file_content called with path: "src\\config.rs" (absolute: false)
    DEBUG: Calculating module path for "src\\config.rs" with root "C:\\Users\\admin\\AppData\\Local\\Temp\\.tmpiwaOME"
    DEBUG: Module path for "src\\config.rs": Some("crate::config")
    DEBUG: Registering file "src\\config.rs" with module path: crate::config
    DEBUG visibility check for create_config: found_visibility=true
      Parent kind: function_item
      Children: ["visibility_modifier", "fn", "identifier", "parameters", "->", "type_identifier", "block"]
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Configured symbol 'create_config' -> module=Some("crate::config::create_config"), visibility=Public
    DEBUG: Configured symbol 'Config' -> module=Some("crate::config::Config"), visibility=Public
    DEBUG: Configured symbol 'value' -> module=Some("crate::config::value"), visibility=Private
    DEBUG: RustParser::find_method_calls override called with enhanced AST detection
    DEBUG: Enhanced method call detection found 1 calls
    DEBUG: Found 1 method calls in file FileId(1)
    DEBUG: Static call: String::new in create_config
    DEBUG: Processing 1 method calls
    DEBUG: Processing call: create_config -> new
    DEBUG: Processing method call with enhanced data: create_config -> new
    DEBUG: Storing method call for enhanced resolution: create_config calls new (static: true, receiver: Some("String"))
    DEBUG: Adding unresolved relationship: create_config -> new (kind: Calls, from_id: Some(SymbolId(1)))
    DEBUG: Found 1 function calls in file FileId(1)
    DEBUG: Processing function call: create_config -> String::new
    DEBUG: Adding unresolved relationship: create_config -> String::new (kind: Calls, from_id: Some(SymbolId(1)))
    DEBUG: Adding unresolved relationship: create_config -> Config (kind: Uses, from_id: Some(SymbolId(1)))
    DEBUG: Adding unresolved relationship: Config -> String (kind: Uses, from_id: Some(SymbolId(2)))
    DEBUG: Found 0 defines for file FileId(1)
    DEBUG: Building symbol cache with 3 symbols at C:\Users\admin\AppData\Local\Temp\.tmpiwaOME\.test_import_resolution\symbol_cache.bin
    DEBUG: Loaded symbol cache from C:\Users\admin\AppData\Local\Temp\.tmpiwaOME\.test_import_resolution\symbol_cache.bin
    DEBUG: Built symbol cache with 3 symbols
    DEBUG: reindex_file_content called with path: "src\\another.rs" (absolute: false)
    DEBUG: Calculating module path for "src\\another.rs" with root "C:\\Users\\admin\\AppData\\Local\\Temp\\.tmpiwaOME"
    DEBUG: Module path for "src\\another.rs": Some("crate::another")
    DEBUG: Registering file "src\\another.rs" with module path: crate::another
    DEBUG visibility check for create_config: found_visibility=true
      Parent kind: function_item
      Children: ["visibility_modifier", "fn", "identifier", "parameters", "->", "type_identifier", "block"]
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: primitive_type
    DEBUG: Configured symbol 'create_config' -> module=Some("crate::another::create_config"), visibility=Public
    DEBUG: Configured symbol 'Another' -> module=Some("crate::another::Another"), visibility=Public
    DEBUG: Configured symbol 'data' -> module=Some("crate::another::data"), visibility=Private
    DEBUG: RustParser::find_method_calls override called with enhanced AST detection
    DEBUG: Enhanced method call detection found 0 calls
    DEBUG: Found 0 method calls in file FileId(2)
    DEBUG: Processing 0 method calls
    DEBUG: Found 0 function calls in file FileId(2)
    DEBUG: Adding unresolved relationship: create_config -> Another (kind: Uses, from_id: Some(SymbolId(4)))
    DEBUG: Adding unresolved relationship: Another -> i32 (kind: Uses, from_id: Some(SymbolId(5)))
    DEBUG: Found 0 defines for file FileId(2)
    (Phase1) commit retry: attempt=1/4 delay=92ms retry_class=Conditional
    DEBUG: Building symbol cache with 3 symbols at C:\Users\admin\AppData\Local\Temp\.tmpiwaOME\.test_import_resolution\symbol_cache.bin
    DEBUG: Loaded symbol cache from C:\Users\admin\AppData\Local\Temp\.tmpiwaOME\.test_import_resolution\symbol_cache.bin
    DEBUG: Built symbol cache with 3 symbols
    DEBUG: reindex_file_content called with path: "src\\main.rs" (absolute: false)
    DEBUG: Calculating module path for "src\\main.rs" with root "C:\\Users\\admin\\AppData\\Local\\Temp\\.tmpiwaOME"
    DEBUG: Module path for "src\\main.rs": Some("crate")
    DEBUG: Registering file "src\\main.rs" with module path: crate
    DEBUG: Found 1 imports in file FileId(1)
    DEBUG:   - Import: crate::config::create_config (alias: None, glob: false)
    DEBUG: Configured symbol 'main' -> module=Some("crate::main"), visibility=Private
    DEBUG: RustParser::find_method_calls override called with enhanced AST detection
    DEBUG: Enhanced method call detection found 1 calls
    DEBUG: Found 1 method calls in file FileId(1)
    DEBUG: Plain call: create_config in main
    DEBUG: Processing 1 method calls
    DEBUG: Processing call: main -> create_config
    DEBUG: Processing method call with enhanced data: main -> create_config
    DEBUG: Storing method call for enhanced resolution: main calls create_config (static: false, receiver: None)
    DEBUG: Adding unresolved relationship: main -> create_config (kind: Calls, from_id: Some(SymbolId(1)))
    DEBUG: Found 1 function calls in file FileId(1)
    DEBUG: Processing function call: main -> create_config
    DEBUG: Adding unresolved relationship: main -> create_config (kind: Calls, from_id: Some(SymbolId(1)))
    DEBUG: Found 0 defines for file FileId(1)
    DEBUG: [find_variable_types_in_node]: Found let_declaration at line 4
    DEBUG:     [extract_value_type] Node kind: 'call_expression', text: 'create_config()'
    DEBUG: Building symbol cache with 4 symbols at C:\Users\admin\AppData\Local\Temp\.tmpiwaOME\.test_import_resolution\symbol_cache.bin
    DEBUG: Loaded symbol cache from C:\Users\admin\AppData\Local\Temp\.tmpiwaOME\.test_import_resolution\symbol_cache.bin
    DEBUG: Built symbol cache with 4 symbols
    Unresolved relationships before resolution: [UnresolvedRelationship { from_id: Some(SymbolId(1)), from_name: "create_config", to_name: "new", file_id: FileId(1), kind: Calls, metadata: Some(RelationshipMetadata { line: Some(2), column: Some(20), context: Some("receiver:String,receiver_norm:String,static:true") }) }, UnresolvedRelationship { from_id: Some(SymbolId(1)), from_name: "create_config", to_name: "String::new", file_id: FileId(1), kind: Calls, metadata: Some(RelationshipMetadata { line: Some(2), column: Some(20), context: Some("function_call") }) }, UnresolvedRelationship { from_id: Some(SymbolId(1)), from_name: "create_config", to_name: "Config", file_id: FileId(1), kind: Uses, metadata: None }, UnresolvedRelationship { from_id: Some(SymbolId(2)), from_name: "Config", to_name: "String", file_id: FileId(1), kind: Uses, metadata: None }, UnresolvedRelationship { from_id: Some(SymbolId(4)), from_name: "create_config", to_name: "Another", file_id: FileId(2), kind: Uses, metadata: None }, UnresolvedRelationship { from_id: Some(SymbolId(5)), from_name: "Another", to_name: "i32", file_id: FileId(2), kind: Uses, metadata: None }, UnresolvedRelationship { from_id: Some(SymbolId(1)), from_name: "main", to_name: "create_config", file_id: FileId(1), kind: Calls, metadata: None }, UnresolvedRelationship { from_id: Some(SymbolId(1)), from_name: "main", to_name: "create_config", file_id: FileId(1), kind: Calls, metadata: Some(RelationshipMetadata { line: Some(4), column: Some(12), context: Some("function_call") }) }]
    Resolving cross-file relationships: 8 unresolved entries
    DEBUG: resolve_cross_file_relationships: 8 unresolved relationships
    DEBUG: Processing relationship: create_config -> new (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(1) for 'create_config'
    DEBUG: Resolving as method call: 'new'
    DEBUG: Found MethodCall object for create_config->new! Using enhanced resolution
    DEBUG: Resolving method call: receiver=String, method=new, is_static=true
    DEBUG: Static method call: String::new
    DEBUG: Static method fallback resolution for new: None
    DEBUG: resolve_method_call_enhanced returned: None for new
    DEBUG: Resolution failed, trying external mapping for new
    DEBUG: Trying to resolve external call target: 'String.new' for file FileId(1)
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'new' from 'create_config' in file FileId(1) (kind: Calls)
    DEBUG: Processing relationship: create_config -> String::new (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(1) for 'create_config'
    DEBUG: Resolving as method call: 'String::new'
    DEBUG: No MethodCall object found for create_config->String::new. Resolving as regular function
    DEBUG: resolve_method_call_enhanced returned: None for String::new
    DEBUG: Resolution failed, trying external mapping for String::new
    DEBUG: Trying to resolve external call target: 'String::new' for file FileId(1)
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'String::new' from 'create_config' in file FileId(1) (kind: Calls)
    DEBUG: Processing relationship: create_config -> Config (kind: Uses, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(1) for 'create_config'
    DEBUG: Resolving relationship: create_config -> Config (kind: Uses)
    DEBUG: Resolution result: Some(SymbolId(2))
    DEBUG: Resolved target symbol 'Config' to ID: SymbolId(2)
    DEBUG: Looking up symbol by ID: SymbolId(2)
    DEBUG: Found target symbol: Config
    DEBUG: Processing 1 from symbols
    DEBUG: Checking relationship from create_config to Config
    DEBUG: Checking visibility: Config (vis: Public, module: Some("crate::config::Config")) from create_config (module: Some("crate::config::create_config"))
    DEBUG: [SUCCESS] Adding relationship: create_config (SymbolId(1)) -> Config (SymbolId(2)) kind: Uses
    DEBUG: Processing relationship: Config -> String (kind: Uses, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(2) for 'Config'
    DEBUG: Resolving relationship: Config -> String (kind: Uses)
    DEBUG: Resolution result: None
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'String' from 'Config' in file FileId(1) (kind: Uses)
    DEBUG: Processing relationship: main -> create_config (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(1) for 'main'
    DEBUG: Resolving as method call: 'create_config'
    DEBUG: Found MethodCall object for main->create_config! Using enhanced resolution
    DEBUG: No receiver found, resolving 'create_config' as regular function
    DEBUG: Regular function resolution result: Some(SymbolId(1))
    DEBUG: resolve_method_call_enhanced returned: Some(SymbolId(1)) for create_config
    DEBUG: Resolved target symbol 'create_config' to ID: SymbolId(1)
    DEBUG: Looking up symbol by ID: SymbolId(1)
    DEBUG: Found target symbol: create_config
    DEBUG: Processing 1 from symbols
    DEBUG: Checking relationship from create_config to create_config
    DEBUG: Checking visibility: create_config (vis: Public, module: Some("crate::config::create_config")) from create_config (module: Some("crate::config::create_config"))
    DEBUG: [SUCCESS] Adding relationship: create_config (SymbolId(1)) -> create_config (SymbolId(1)) kind: Calls
    DEBUG: Processing relationship: main -> create_config (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(1) for 'main'
    DEBUG: Resolving as method call: 'create_config'
    DEBUG: Found MethodCall object for main->create_config! Using enhanced resolution
    DEBUG: No receiver found, resolving 'create_config' as regular function
    DEBUG: Regular function resolution result: Some(SymbolId(1))
    DEBUG: resolve_method_call_enhanced returned: Some(SymbolId(1)) for create_config
    DEBUG: Resolved target symbol 'create_config' to ID: SymbolId(1)
    DEBUG: Looking up symbol by ID: SymbolId(1)
    DEBUG: Found target symbol: create_config
    DEBUG: Processing 1 from symbols
    DEBUG: Checking relationship from create_config to create_config
    DEBUG: Checking visibility: create_config (vis: Public, module: Some("crate::config::create_config")) from create_config (module: Some("crate::config::create_config"))
    DEBUG: [SUCCESS] Adding relationship: create_config (SymbolId(1)) -> create_config (SymbolId(1)) kind: Calls
    DEBUG: Processing relationship: create_config -> Another (kind: Uses, file: FileId(2))
    DEBUG: Using cached from_id: SymbolId(4) for 'create_config'
    DEBUG: WARNING: from_id SymbolId(4) not found for 'create_config'
    DEBUG: Resolving relationship: create_config -> Another (kind: Uses)
    DEBUG: Resolution result: Some(SymbolId(5))
    DEBUG: Resolved target symbol 'Another' to ID: SymbolId(5)
    DEBUG: Looking up symbol by ID: SymbolId(5)
    DEBUG: Found target symbol: Another
    DEBUG: Processing 0 from symbols
    DEBUG: Processing relationship: Another -> i32 (kind: Uses, file: FileId(2))
    DEBUG: Using cached from_id: SymbolId(5) for 'Another'
    DEBUG: Resolving relationship: Another -> i32 (kind: Uses)
    DEBUG: Resolution result: None
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'i32' from 'Another' in file FileId(2) (kind: Uses)
    Progress: [] 100%
    8/8 relationships | 3 resolved | 4 skipped | 518/s | 0.0s
    DEBUG: Building symbol cache with 4 symbols at C:\Users\admin\AppData\Local\Temp\.tmpiwaOME\.test_import_resolution\symbol_cache.bin
    DEBUG: Loaded symbol cache from C:\Users\admin\AppData\Local\Temp\.tmpiwaOME\.test_import_resolution\symbol_cache.bin
    DEBUG: Built symbol cache with 4 symbols
    Progress: [] 100%
    8/8 relationships | 3 resolved | 4 skipped | 26/s | 0.3s
    DEBUG: Relationship resolution complete - resolved: 3, skipped: 4, total: 8

    thread 'indexing::simple::tests::test_import_based_relationship_resolution' (33020) panicked at src\indexing\simple.rs:3896:9:
    assertion `left == right` failed
      left: 0
     right: 1
    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

  Cancelling due to test failure: 11 tests still running
        PASS [   1.118s] codanna indexing::simple::tests::test_symbols_in_same_module
        PASS [   2.946s] codanna indexing::simple::tests::test_real_php_resolution_integration
        PASS [   2.881s] codanna indexing::simple::tests::test_real_python_resolution_integration
        FAIL [   3.895s] codanna indexing::simple::tests::test_find_symbols_with_language_filter
  stdout 

    running 1 test

    === Testing SimpleIndexer with language filtering ===
    Test 1 - No filter: Found 2 'process_data' symbols
      - Language: Some(LanguageId("typescript")), File: main.ts
      - Language: Some(LanguageId("rust")), File: main.rs
    test indexing::simple::tests::test_find_symbols_with_language_filter ... FAILED

    failures:

    failures:
        indexing::simple::tests::test_find_symbols_with_language_filter

    test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 558 filtered out; finished in 3.04s

  stderr 
    Resolving cross-file relationships: 0 unresolved entries
    DEBUG: No unresolved relationships to process
    (Phase1) commit retry: attempt=1/4 delay=98ms retry_class=Conditional
    (Phase1) commit retry: attempt=2/4 delay=122ms retry_class=Conditional
    (Phase1) commit retry: attempt=3/4 delay=210ms retry_class=Conditional
    Resolving cross-file relationships: 1 unresolved entries
    Progress: [] 100%
    1/1 relationships | 1 skipped | 52/s | 0.0s
    Progress: [] 100%
    1/1 relationships | 1 skipped | 3/s | 0.3s
    Resolving cross-file relationships: 2 unresolved entries
    Progress: [] 100%
    2/2 relationships | 2 skipped | 570/s | 0.0s
    Progress: [] 100%
    2/2 relationships | 2 skipped | 55/s | 0.0s

    thread 'indexing::simple::tests::test_find_symbols_with_language_filter' (30972) panicked at src\indexing\simple.rs:4597:9:
    assertion `left == right` failed: Should find 3 process_data functions
      left: 2
     right: 3
    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   3.093s] codanna indexing::simple::tests::test_real_typescript_resolution_integration
        PASS [   3.191s] codanna indexing::simple::tests::test_real_relationship_resolution_integration
        FAIL [   3.116s] codanna indexing::simple::tests::test_real_rust_resolution_integration
  stdout 

    running 1 test

    === REAL TDD: Rust Resolution Integration Test ===
    Writing REAL Rust code to file:
    use std::fmt::Display;

    trait Logger {
        fn log(&self, message: &str);
        fn warn(&self, message: &str);
    }

    struct DatabaseLogger {
        connection: String,
    }

    impl DatabaseLogger {
        fn new(connection: String) -> Self {
            DatabaseLogger { connection }
        }

        fn connect(&self) -> bool {
            !self.connection.is_empty()
        }
    }

    impl Logger for DatabaseLogger {
        fn log(&self, message: &str) {
            println!("[DB] {}", message);
        }

        fn warn(&self, message: &str) {
            println!("[DB WARNING] {}", message);
        }
    }

    impl Display for DatabaseLogger {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "DatabaseLogger({})", self.connection)
        }
    }

    fn main() {
        let logger = DatabaseLogger::new("localhost:5432".to_string());
        logger.log("Application started");
        let connected = logger.connect();
    }


    --- STEP 1: Indexing real Rust code file ---
    test indexing::simple::tests::test_real_rust_resolution_integration ... FAILED

    failures:

    failures:
        indexing::simple::tests::test_real_rust_resolution_integration

    test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 558 filtered out; finished in 2.50s

  stderr 
    DEBUG: No symbol cache found at C:\Users\admin\AppData\Local\Temp\.tmpL03QRJ\.codanna-test-local\index\symbol_cache.bin
    DEBUG: reindex_file_content called with path: "test_code.rs" (absolute: false)
    DEBUG: Calculating module path for "test_code.rs" with root "C:\\Users\\admin\\AppData\\Local\\Temp\\.tmpL03QRJ"
    DEBUG: Module path for "test_code.rs": Some("crate::test_code")
    DEBUG: Registering file "test_code.rs" with module path: crate::test_code
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: primitive_type
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found node kind: scoped_type_identifier
    DEBUG: Found node kind: type_identifier
    DEBUG: Found 1 imports in file FileId(1)
    DEBUG:   - Import: std::fmt::Display (alias: None, glob: false)
    DEBUG: Configured symbol 'Logger' -> module=Some("crate::test_code::Logger"), visibility=Private
    DEBUG: Configured symbol 'log' -> module=Some("crate::test_code::log"), visibility=Private
    DEBUG: Configured symbol 'warn' -> module=Some("crate::test_code::warn"), visibility=Private
    DEBUG: Configured symbol 'DatabaseLogger' -> module=Some("crate::test_code::DatabaseLogger"), visibility=Private
    DEBUG: Configured symbol 'connection' -> module=Some("crate::test_code::connection"), visibility=Private
    DEBUG: Configured symbol 'new' -> module=Some("crate::test_code::new"), visibility=Private
    DEBUG: Configured symbol 'connect' -> module=Some("crate::test_code::connect"), visibility=Private
    DEBUG: Configured symbol 'log' -> module=Some("crate::test_code::log"), visibility=Private
    DEBUG: Configured symbol 'warn' -> module=Some("crate::test_code::warn"), visibility=Private
    DEBUG: Configured symbol 'fmt' -> module=Some("crate::test_code::fmt"), visibility=Private
    DEBUG: Configured symbol 'main' -> module=Some("crate::test_code::main"), visibility=Private
    DEBUG: RustParser::find_method_calls override called with enhanced AST detection
    DEBUG: Enhanced method call detection found 5 calls
    DEBUG: Found 5 method calls in file FileId(1)
    DEBUG: Instance call: self.connection.is_empty in connect (receiver will be lost in current format)
    DEBUG: Static call: DatabaseLogger::new in main
    DEBUG: Instance call: "localhost:5432".to_string in main (receiver will be lost in current format)
    DEBUG: Instance call: logger.log in main (receiver will be lost in current format)
    DEBUG: Instance call: logger.connect in main (receiver will be lost in current format)
    DEBUG: Processing 5 method calls
    DEBUG: Processing call: connect -> is_empty
    DEBUG: Processing method call with enhanced data: connect -> is_empty
    DEBUG: Storing method call for enhanced resolution: connect calls is_empty (static: false, receiver: Some("self.connection"))
    DEBUG: Adding unresolved relationship: connect -> is_empty (kind: Calls, from_id: Some(SymbolId(7)))
    DEBUG: Processing call: main -> new
    DEBUG: Processing method call with enhanced data: main -> new
    DEBUG: Storing method call for enhanced resolution: main calls new (static: true, receiver: Some("DatabaseLogger"))
    DEBUG: Adding unresolved relationship: main -> new (kind: Calls, from_id: Some(SymbolId(11)))
    DEBUG: Processing call: main -> to_string
    DEBUG: Processing method call with enhanced data: main -> to_string
    DEBUG: Storing method call for enhanced resolution: main calls to_string (static: false, receiver: Some("\"localhost:5432\""))
    DEBUG: Adding unresolved relationship: main -> to_string (kind: Calls, from_id: Some(SymbolId(11)))
    DEBUG: Processing call: main -> log
    DEBUG: Processing method call with enhanced data: main -> log
    DEBUG: Storing method call for enhanced resolution: main calls log (static: false, receiver: Some("logger"))
    DEBUG: Adding unresolved relationship: main -> log (kind: Calls, from_id: Some(SymbolId(11)))
    DEBUG: Processing call: main -> connect
    DEBUG: Processing method call with enhanced data: main -> connect
    DEBUG: Storing method call for enhanced resolution: main calls connect (static: false, receiver: Some("logger"))
    DEBUG: Adding unresolved relationship: main -> connect (kind: Calls, from_id: Some(SymbolId(11)))
    DEBUG: Found 5 function calls in file FileId(1)
    DEBUG: Processing function call: connect -> is_empty
    DEBUG: Adding unresolved relationship: connect -> is_empty (kind: Calls, from_id: Some(SymbolId(7)))
    DEBUG: Processing function call: main -> DatabaseLogger::new
    DEBUG: Adding unresolved relationship: main -> DatabaseLogger::new (kind: Calls, from_id: Some(SymbolId(11)))
    DEBUG: Processing function call: main -> to_string
    DEBUG: Adding unresolved relationship: main -> to_string (kind: Calls, from_id: Some(SymbolId(11)))
    DEBUG: Processing function call: main -> log
    DEBUG: Adding unresolved relationship: main -> log (kind: Calls, from_id: Some(SymbolId(11)))
    DEBUG: Processing function call: main -> connect
    DEBUG: Adding unresolved relationship: main -> connect (kind: Calls, from_id: Some(SymbolId(11)))
    DEBUG: Registering implementation: DatabaseLogger implements Logger
    DEBUG: Adding unresolved relationship: DatabaseLogger -> Logger (kind: Implements, from_id: Some(SymbolId(4)))
    DEBUG: Registering implementation: DatabaseLogger implements Display
    DEBUG: Adding unresolved relationship: DatabaseLogger -> Display (kind: Implements, from_id: Some(SymbolId(4)))
    DEBUG: Found inherent method: DatabaseLogger::new
    DEBUG: Found inherent method: DatabaseLogger::connect
    DEBUG: Adding unresolved relationship: DatabaseLogger -> String (kind: Uses, from_id: Some(SymbolId(4)))
    DEBUG: Adding unresolved relationship: new -> String (kind: Uses, from_id: Some(SymbolId(6)))
    DEBUG: Adding unresolved relationship: new -> Self (kind: Uses, from_id: Some(SymbolId(6)))
    DEBUG: Adding unresolved relationship: connect -> bool (kind: Uses, from_id: Some(SymbolId(7)))
    DEBUG: Adding unresolved relationship: log -> str (kind: Uses, from_id: Some(SymbolId(8)))
    DEBUG: Adding unresolved relationship: warn -> str (kind: Uses, from_id: Some(SymbolId(9)))
    DEBUG: Adding unresolved relationship: fmt -> std::fmt::Formatter (kind: Uses, from_id: Some(SymbolId(10)))
    DEBUG: Adding unresolved relationship: fmt -> std::fmt::Result (kind: Uses, from_id: Some(SymbolId(10)))
    DEBUG: Found 7 defines for file FileId(1)
    DEBUG: Processing define: Logger defines log
    DEBUG: Found 9 symbol kinds for file
    DEBUG: Found kind Trait for definer Logger
    DEBUG: Adding method 'log' to trait 'Logger'
    DEBUG: Adding unresolved relationship: Logger -> log (kind: Defines, from_id: Some(SymbolId(1)))
    DEBUG: Processing define: Logger defines warn
    DEBUG: Found 9 symbol kinds for file
    DEBUG: Found kind Trait for definer Logger
    DEBUG: Adding method 'warn' to trait 'Logger'
    DEBUG: Adding unresolved relationship: Logger -> warn (kind: Defines, from_id: Some(SymbolId(1)))
    DEBUG: Processing define: DatabaseLogger defines new
    DEBUG: Found 9 symbol kinds for file
    DEBUG: Found kind Struct for definer DatabaseLogger
    DEBUG: Adding unresolved relationship: DatabaseLogger -> new (kind: Defines, from_id: Some(SymbolId(4)))
    DEBUG: Processing define: DatabaseLogger defines connect
    DEBUG: Found 9 symbol kinds for file
    DEBUG: Found kind Struct for definer DatabaseLogger
    DEBUG: Adding unresolved relationship: DatabaseLogger -> connect (kind: Defines, from_id: Some(SymbolId(4)))
    DEBUG: Processing define: DatabaseLogger defines log
    DEBUG: Found 9 symbol kinds for file
    DEBUG: Found kind Struct for definer DatabaseLogger
    DEBUG: Adding unresolved relationship: DatabaseLogger -> log (kind: Defines, from_id: Some(SymbolId(4)))
    DEBUG: Processing define: DatabaseLogger defines warn
    DEBUG: Found 9 symbol kinds for file
    DEBUG: Found kind Struct for definer DatabaseLogger
    DEBUG: Adding unresolved relationship: DatabaseLogger -> warn (kind: Defines, from_id: Some(SymbolId(4)))
    DEBUG: Processing define: DatabaseLogger defines fmt
    DEBUG: Found 9 symbol kinds for file
    DEBUG: Found kind Struct for definer DatabaseLogger
    DEBUG: Adding unresolved relationship: DatabaseLogger -> fmt (kind: Defines, from_id: Some(SymbolId(4)))
    DEBUG: [find_variable_types_in_node]: Found let_declaration at line 38
    DEBUG:     [extract_value_type] Node kind: 'call_expression', text: 'DatabaseLogger::new("localhost:5432".to_string())'
    DEBUG: [find_variable_types_in_node]: Found let_declaration at line 40
    DEBUG:     [extract_value_type] Node kind: 'call_expression', text: 'logger.connect()'
    DEBUG: Building symbol cache with 11 symbols at C:\Users\admin\AppData\Local\Temp\.tmpL03QRJ\.codanna-test-local\index\symbol_cache.bin
    DEBUG: Loaded symbol cache from C:\Users\admin\AppData\Local\Temp\.tmpL03QRJ\.codanna-test-local\index\symbol_cache.bin
    DEBUG: Built symbol cache with 11 symbols
    Resolving cross-file relationships: 27 unresolved entries
    DEBUG: resolve_cross_file_relationships: 27 unresolved relationships
    Progress: [                            ]   0%
    0/27 relationships | 0/s | 0.0s
    DEBUG: Processing relationship: connect -> is_empty (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(7) for 'connect'
    DEBUG: Resolving as method call: 'is_empty'
    DEBUG: Found MethodCall object for connect->is_empty! Using enhanced resolution
    DEBUG: Resolving method call: receiver=self.connection, method=is_empty, is_static=false
    DEBUG: resolve_method_call_enhanced returned: None for is_empty
    DEBUG: Resolution failed, trying external mapping for is_empty
    DEBUG: Trying to resolve external call target: 'self.connection.is_empty' for file FileId(1)
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'is_empty' from 'connect' in file FileId(1) (kind: Calls)
    DEBUG: Processing relationship: main -> new (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(11) for 'main'
    DEBUG: Resolving as method call: 'new'
    DEBUG: Found MethodCall object for main->new! Using enhanced resolution
    DEBUG: Resolving method call: receiver=DatabaseLogger, method=new, is_static=true
    DEBUG: Static method call: DatabaseLogger::new
    DEBUG: Resolved static method DatabaseLogger::new to symbol_id: SymbolId(6)
    DEBUG: resolve_method_call_enhanced returned: Some(SymbolId(6)) for new
    DEBUG: Resolved target symbol 'new' to ID: SymbolId(6)
    DEBUG: Looking up symbol by ID: SymbolId(6)
    DEBUG: Found target symbol: new
    DEBUG: Processing 1 from symbols
    DEBUG: Checking relationship from main to new
    DEBUG: Checking visibility: new (vis: Private, module: Some("crate::test_code::new")) from main (module: Some("crate::test_code::main"))
    DEBUG: [SUCCESS] Adding relationship: main (SymbolId(11)) -> new (SymbolId(6)) kind: Calls
    DEBUG: Processing relationship: main -> to_string (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(11) for 'main'
    DEBUG: Resolving as method call: 'to_string'
    DEBUG: Found MethodCall object for main->to_string! Using enhanced resolution
    DEBUG: Resolving method call: receiver="localhost:5432", method=to_string, is_static=false
    DEBUG: resolve_method_call_enhanced returned: None for to_string
    DEBUG: Resolution failed, trying external mapping for to_string
    DEBUG: Trying to resolve external call target: '"localhost:5432".to_string' for file FileId(1)
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'to_string' from 'main' in file FileId(1) (kind: Calls)
    DEBUG: Processing relationship: main -> log (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(11) for 'main'
    DEBUG: Resolving as method call: 'log'
    DEBUG: Found MethodCall object for main->log! Using enhanced resolution
    DEBUG: Resolving method call: receiver=logger, method=log, is_static=false
    DEBUG: resolve_method_call_enhanced returned: None for log
    DEBUG: Resolution failed, trying external mapping for log
    DEBUG: Trying to resolve external call target: 'logger.log' for file FileId(1)
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'log' from 'main' in file FileId(1) (kind: Calls)
    DEBUG: Processing relationship: main -> connect (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(11) for 'main'
    DEBUG: Resolving as method call: 'connect'
    DEBUG: Found MethodCall object for main->connect! Using enhanced resolution
    DEBUG: Resolving method call: receiver=logger, method=connect, is_static=false
    DEBUG: resolve_method_call_enhanced returned: None for connect
    DEBUG: Resolution failed, trying external mapping for connect
    DEBUG: Trying to resolve external call target: 'logger.connect' for file FileId(1)
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'connect' from 'main' in file FileId(1) (kind: Calls)
    DEBUG: Processing relationship: connect -> is_empty (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(7) for 'connect'
    DEBUG: Resolving as method call: 'is_empty'
    DEBUG: Found MethodCall object for connect->is_empty! Using enhanced resolution
    DEBUG: Resolving method call: receiver=self.connection, method=is_empty, is_static=false
    DEBUG: resolve_method_call_enhanced returned: None for is_empty
    DEBUG: Resolution failed, trying external mapping for is_empty
    DEBUG: Trying to resolve external call target: 'self.connection.is_empty' for file FileId(1)
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'is_empty' from 'connect' in file FileId(1) (kind: Calls)
    DEBUG: Processing relationship: main -> DatabaseLogger::new (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(11) for 'main'
    DEBUG: Resolving as method call: 'DatabaseLogger::new'
    DEBUG: No MethodCall object found for main->DatabaseLogger::new. Resolving as regular function
    DEBUG: resolve_method_call_enhanced returned: Some(SymbolId(6)) for DatabaseLogger::new
    DEBUG: Resolved target symbol 'DatabaseLogger::new' to ID: SymbolId(6)
    DEBUG: Looking up symbol by ID: SymbolId(6)
    DEBUG: Found target symbol: new
    DEBUG: Processing 1 from symbols
    DEBUG: Checking relationship from main to new
    DEBUG: Checking visibility: new (vis: Private, module: Some("crate::test_code::new")) from main (module: Some("crate::test_code::main"))
    DEBUG: [SUCCESS] Adding relationship: main (SymbolId(11)) -> new (SymbolId(6)) kind: Calls
    DEBUG: Processing relationship: main -> to_string (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(11) for 'main'
    DEBUG: Resolving as method call: 'to_string'
    DEBUG: Found MethodCall object for main->to_string! Using enhanced resolution
    DEBUG: Resolving method call: receiver="localhost:5432", method=to_string, is_static=false
    DEBUG: resolve_method_call_enhanced returned: None for to_string
    DEBUG: Resolution failed, trying external mapping for to_string
    DEBUG: Trying to resolve external call target: '"localhost:5432".to_string' for file FileId(1)
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'to_string' from 'main' in file FileId(1) (kind: Calls)
    DEBUG: Processing relationship: main -> log (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(11) for 'main'
    DEBUG: Resolving as method call: 'log'
    DEBUG: Found MethodCall object for main->log! Using enhanced resolution
    DEBUG: Resolving method call: receiver=logger, method=log, is_static=false
    DEBUG: resolve_method_call_enhanced returned: None for log
    DEBUG: Resolution failed, trying external mapping for log
    DEBUG: Trying to resolve external call target: 'logger.log' for file FileId(1)
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'log' from 'main' in file FileId(1) (kind: Calls)
    DEBUG: Processing relationship: main -> connect (kind: Calls, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(11) for 'main'
    DEBUG: Resolving as method call: 'connect'
    DEBUG: Found MethodCall object for main->connect! Using enhanced resolution
    DEBUG: Resolving method call: receiver=logger, method=connect, is_static=false
    DEBUG: resolve_method_call_enhanced returned: None for connect
    DEBUG: Resolution failed, trying external mapping for connect
    DEBUG: Trying to resolve external call target: 'logger.connect' for file FileId(1)
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'connect' from 'main' in file FileId(1) (kind: Calls)
    DEBUG: Processing relationship: DatabaseLogger -> Logger (kind: Implements, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(4) for 'DatabaseLogger'
    DEBUG: Resolving relationship: DatabaseLogger -> Logger (kind: Implements)
    DEBUG: Resolution result: Some(SymbolId(1))
    DEBUG: Resolved target symbol 'Logger' to ID: SymbolId(1)
    DEBUG: Looking up symbol by ID: SymbolId(1)
    DEBUG: Found target symbol: Logger
    DEBUG: Processing 1 from symbols
    DEBUG: Checking relationship from DatabaseLogger to Logger
    DEBUG: Checking visibility: Logger (vis: Private, module: Some("crate::test_code::Logger")) from DatabaseLogger (module: Some("crate::test_code::DatabaseLogger"))
    DEBUG: [SUCCESS] Adding relationship: DatabaseLogger (SymbolId(4)) -> Logger (SymbolId(1)) kind: Implements
    DEBUG: Processing relationship: DatabaseLogger -> Display (kind: Implements, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(4) for 'DatabaseLogger'
    DEBUG: Resolving relationship: DatabaseLogger -> Display (kind: Implements)
    DEBUG: Resolution result: None
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'Display' from 'DatabaseLogger' in file FileId(1) (kind: Implements)
    DEBUG: Processing relationship: DatabaseLogger -> String (kind: Uses, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(4) for 'DatabaseLogger'
    DEBUG: Resolving relationship: DatabaseLogger -> String (kind: Uses)
    DEBUG: Resolution result: None
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'String' from 'DatabaseLogger' in file FileId(1) (kind: Uses)
    DEBUG: Processing relationship: new -> String (kind: Uses, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(6) for 'new'
    DEBUG: Resolving relationship: new -> String (kind: Uses)
    DEBUG: Resolution result: None
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'String' from 'new' in file FileId(1) (kind: Uses)
    DEBUG: Processing relationship: new -> Self (kind: Uses, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(6) for 'new'
    DEBUG: Resolving relationship: new -> Self (kind: Uses)
    DEBUG: Resolution result: None
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'Self' from 'new' in file FileId(1) (kind: Uses)
    DEBUG: Processing relationship: connect -> bool (kind: Uses, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(7) for 'connect'
    DEBUG: Resolving relationship: connect -> bool (kind: Uses)
    DEBUG: Resolution result: None
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'bool' from 'connect' in file FileId(1) (kind: Uses)
    DEBUG: Processing relationship: log -> str (kind: Uses, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(8) for 'log'
    DEBUG: Resolving relationship: log -> str (kind: Uses)
    DEBUG: Resolution result: None
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'str' from 'log' in file FileId(1) (kind: Uses)
    DEBUG: Processing relationship: warn -> str (kind: Uses, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(9) for 'warn'
    DEBUG: Resolving relationship: warn -> str (kind: Uses)
    DEBUG: Resolution result: None
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'str' from 'warn' in file FileId(1) (kind: Uses)
    DEBUG: Processing relationship: fmt -> std::fmt::Formatter (kind: Uses, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(10) for 'fmt'
    DEBUG: Resolving relationship: fmt -> std::fmt::Formatter (kind: Uses)
    DEBUG: Resolution result: None
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'std::fmt::Formatter' from 'fmt' in file FileId(1) (kind: Uses)
    DEBUG: Processing relationship: fmt -> std::fmt::Result (kind: Uses, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(10) for 'fmt'
    DEBUG: Resolving relationship: fmt -> std::fmt::Result (kind: Uses)
    DEBUG: Resolution result: None
    DEBUG: [SKIP-RESOLUTION] Failed to resolve 'std::fmt::Result' from 'fmt' in file FileId(1) (kind: Uses)
    DEBUG: Processing relationship: Logger -> log (kind: Defines, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(1) for 'Logger'
    DEBUG: Resolving relationship: Logger -> log (kind: Defines)
    DEBUG: Resolution result: Some(SymbolId(2))
    DEBUG: Resolved target symbol 'log' to ID: SymbolId(2)
    DEBUG: Looking up symbol by ID: SymbolId(2)
    DEBUG: Found target symbol: log
    DEBUG: Processing 1 from symbols
    DEBUG: Checking relationship from Logger to log
    DEBUG: [SUCCESS] Adding relationship: Logger (SymbolId(1)) -> log (SymbolId(2)) kind: Defines
    DEBUG: Processing relationship: Logger -> warn (kind: Defines, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(1) for 'Logger'
    DEBUG: Resolving relationship: Logger -> warn (kind: Defines)
    DEBUG: Resolution result: Some(SymbolId(3))
    DEBUG: Resolved target symbol 'warn' to ID: SymbolId(3)
    DEBUG: Looking up symbol by ID: SymbolId(3)
    DEBUG: Found target symbol: warn
    DEBUG: Processing 1 from symbols
    DEBUG: Checking relationship from Logger to warn
    DEBUG: [SUCCESS] Adding relationship: Logger (SymbolId(1)) -> warn (SymbolId(3)) kind: Defines
    DEBUG: Processing relationship: DatabaseLogger -> new (kind: Defines, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(4) for 'DatabaseLogger'
    DEBUG: Resolving relationship: DatabaseLogger -> new (kind: Defines)
    DEBUG: Resolution result: Some(SymbolId(6))
    DEBUG: Resolved target symbol 'new' to ID: SymbolId(6)
    DEBUG: Looking up symbol by ID: SymbolId(6)
    DEBUG: Found target symbol: new
    DEBUG: Processing 1 from symbols
    DEBUG: Checking relationship from DatabaseLogger to new
    DEBUG: [SUCCESS] Adding relationship: DatabaseLogger (SymbolId(4)) -> new (SymbolId(6)) kind: Defines
    DEBUG: Processing relationship: DatabaseLogger -> connect (kind: Defines, file: FileId(1))
    DEBUG: Using cached from_id: SymbolId(4) for 'DatabaseLogger'
    DEBUG: Resolving relationship: DatabaseLogger -> connect (kind: Defines)
    DEBUG: Resolution result: Some(SymbolId(7))
    DEBUG: Resolved target symbol 'connect' to ID: SymbolId(7)
    DEBUG: Looking up symbol by ID: SymbolId(7)
    DEBUG: Found target symbol: connect
    DEBUG: Processing 1 from symbols
    DEBUG: Checking relationship from DatabaseLogger to connect
    DEBUG: [SUCCESS] Adding relationship: DatabaseLogger (SymbolId(4)) -> connect (SymbolId(7)) kind: Defines

    thread 'indexing::simple::tests::test_real_rust_resolution_integration' (16488) panicked at src\indexing\simple.rs:5760:14:
    Failed to index file: TantivyError { operation: "store_relationship", cause: "Tantivy error: An error occurred in a thread: 'An index writer was killed.. A worker thread encountered an error (io::Error most likely) or panicked.'" }
    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        PASS [   3.095s] codanna indexing::simple::tests::test_symbol_module_paths
        PASS [   2.997s] codanna indexing::simple::tests::test_trait_implementations_resolution
        PASS [   3.579s] codanna indexing::simple::tests::test_search_with_language_filter
        PASS [   3.615s] codanna indexing::simple::tests::test_symbols_get_language_id_during_indexing

     Summary [   6.672s] 60/771 tests run: 57 passed, 3 failed, 21 skipped
        FAIL [   2.283s] codanna indexing::simple::tests::test_import_based_relationship_resolution
        FAIL [   3.895s] codanna indexing::simple::tests::test_find_symbols_with_language_filter
        FAIL [   3.116s] codanna indexing::simple::tests::test_real_rust_resolution_integration
warning: 711/771 tests were not run due to test failure (run with --no-fail-fast to run all tests, or run with --max-fail)
error: test run failed

admin@ab002665 MSYS /c/Drive/rust/codanna (main)
$