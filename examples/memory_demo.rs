// 记忆系统使用示例
// 展示如何在 Agent 中集成和使用记忆系统

use std::path::PathBuf;

use openhermes_memory::{MemoryManager, BuiltinMemoryProvider, database::MemoryDatabase};
use openhermes_tools::{init_tools, REGISTRY};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🧠 OpenHermes 记忆系统示例\n");

    // 1. 初始化工具系统（包含记忆工具）
    println!("1️⃣  初始化工具系统...");
    init_tools();
    
    let tool_count = REGISTRY.get_all_tool_names().len();
    println!("   ✅ 已加载 {} 个工具\n", tool_count);

    // 2. 创建数据库
    println!("2️⃣  创建记忆数据库...");
    let db_path = PathBuf::from("example_memory.db");
    let database = MemoryDatabase::new(&db_path).await?;
    println!("   ✅ 数据库创建成功: {}\n", db_path.display());

    // 3. 创建记忆管理器
    println!("3️⃣  创建记忆管理器...");
    let mut manager = MemoryManager::with_database(db_path.clone()).await?;
    
    // 添加内置提供者
    let provider = BuiltinMemoryProvider::new();
    manager.add_provider("builtin", Box::new(provider)).await;
    println!("   ✅ 记忆管理器初始化成功\n");

    // 4. 写入记忆示例
    println!("4️⃣  写入记忆...");
    
    // 记忆工具可以通过 REGISTRY 调用
    let write_result = REGISTRY.execute("memory_write", serde_json::json!({
        "key": "user_preference_language",
        "value": "用户偏好使用中文交流",
        "category": "preferences",
        "tags": ["language", "communication"],
        "importance": 0.8
    })).await?;
    
    println!("   ✅ 写入结果: {}\n", write_result);

    // 5. 读取记忆示例
    println!("5️⃣  读取记忆...");
    
    let read_result = REGISTRY.execute("memory_read", serde_json::json!({
        "query": "用户偏好",
        "category": "preferences",
        "limit": 5
    })).await?;
    
    println!("   ✅ 读取结果:\n   {}\n", read_result);

    // 6. 搜索会话示例
    println!("6️⃣  搜索会话历史...");
    
    let search_result = REGISTRY.execute("memory_search", serde_json::json!({
        "query": "Rust 编程",
        "limit": 3
    })).await?;
    
    println!("   ✅ 搜索结果:\n   {}\n", search_result);

    // 7. FTS5 全文搜索演示
    println!("7️⃣  FTS5 全文搜索语法演示...");
    
    let test_queries = vec![
        "用户偏好",
        "Rust 编程 异步",
        "language communication",
        "preferences tags",
    ];
    
    for query in test_queries {
        let fts_query = openhermes_memory::fts5::FTSSearch::prepare_query(query);
        println!("   原始查询: {} → FTS5: {}", query, fts_query);
    }
    println!();

    // 8. 展示记忆管理功能
    println!("8️⃣  记忆管理...");
    
    // 预取相关记忆（对话前调用）
    let context = manager.prefetch_all("用户想要学习 Rust 编程").await;
    println!("   ✅ 预取上下文长度: {} 字符\n", context.len());

    // 同步记忆（对话后调用）
    manager.sync_all(
        "用户想要学习 Rust 编程",
        "推荐从异步编程和所有权系统开始学习"
    ).await;
    println!("   ✅ 记忆同步完成\n");

    println!("✨ 记忆系统示例完成！");
    println!("\n📊 工具列表:");
    println!("   - memory_read:   搜索已存储的记忆");
    println!("   - memory_write:  写入新的记忆");
    println!("   - memory_search: 搜索会话历史");
    println!("\n💡 提示: 数据库已保存在 {}", db_path.display());
    println!("   可以使用 SQLite 浏览器查看: sqlitebrowser {}", db_path.display());

    // 清理示例数据库（可选）
    // std::fs::remove_file(&db_path)?;
    
    Ok(())
}
