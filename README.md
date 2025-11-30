# MySQL Compare

一个用Rust编写的MySQL数据库结构对比工具，用于比较两个MySQL数据库的表结构差异并生成ALTER SQL语句。

## 功能特性

- 🔍 **数据库结构对比**: 对比两个MySQL数据库的表定义
- 📄 **SQL文件对比**: 支持对SQL文件中的表结构进行对比
- 🛠️ **自动生成ALTER语句**: 自动生成从目标数据库迁移到源数据库的ALTER语句
- 📊 **详细的差异分析**:
  - 列定义变更（添加、修改、删除）
  - 约束变更（主键、唯一键、普通索引、全文索引、外键）
  - 表选项变更（引擎、字符集）

## 安装

### 系统要求

- Rust 1.70 或更高版本
- MySQL 5.7+ 或 MariaDB（当使用数据库模式时）

### 编译

```bash
cargo build --release
```

编译完成后，可执行文件位于 `target/release/mysql_compare`

## 使用方法

### 基本用法

```bash
mysql_compare -d <data_source> -s <source> -t <target> -o <output>
```

### 参数说明

| 参数 | 简写 | 说明 | 必需 |
|------|------|------|------|
| `--data` | `-d` | 数据源类型，可选值：`file` (SQL文件) 或 `db` (数据库连接) | ✓ |
| `--source` | `-s` | 源数据库/文件路径 | ✓ |
| `--target` | `-t` | 目标数据库/文件路径 | ✓ |
| `--output` | `-o` | 输出SQL文件路径 | ✓ |

### 使用示例

#### 示例1：对比两个SQL文件

```bash
mysql_compare -d file -s source.sql -t target.sql -o diff.sql
```

#### 示例2：对比数据库和SQL文件

```bash
mysql_compare -d db -s user:password@localhost:3306~source_db -t target.sql -o diff.sql
```

#### 示例3：对比两个数据库

```bash
mysql_compare -d db \
  -s user:password@localhost:3306~source_db \
  -t user:password@localhost:3306~target_db \
  -o diff.sql
```

### 数据库连接字符串格式

当使用 `-d db` 时，连接字符串格式为：

```
user:password@host:port~database_name
```

**示例**：

```
root:123456@192.168.1.100:3306~mydb
```

## 输出示例

生成的SQL文件示例：

```sql
-- set default character
SET NAMES utf8;

-- users
ALTER TABLE `users`
ADD COLUMN `email` varchar(255) NOT NULL,
MODIFY COLUMN `name` varchar(100) NOT NULL,
DROP COLUMN `old_field`,
ADD UNIQUE INDEX `email_idx` (`email`);

-- products
CREATE TABLE `products` (
  `id` bigint NOT NULL AUTO_INCREMENT,
  `name` varchar(255) NOT NULL,
  `price` decimal(10,2) NOT NULL,
  PRIMARY KEY (`id`),
  KEY `name_idx` (`name`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
```

## 项目结构

```
mysql_compare/
├── Cargo.toml          # 项目配置文件
├── Cargo.lock          # 依赖锁定文件
├── README.md           # 本文件
└── src/
    └── main.rs         # 项目主文件
```

## 依赖

- **mysql**: MySQL数据库连接和操作库
- **clap**: 命令行参数解析
- **anyhow**: 错误处理
- **regex**: 正则表达式处理

## 工作原理

1. **数据源读取**：
   - 从数据库连接或SQL文件中读取表定义
   - 支持多个表的并发读取

2. **结构解析**：
   - 使用正则表达式解析CREATE TABLE语句
   - 提取列定义、约束、表选项等信息

3. **差异对比**：
   - 逐表进行结构对比
   - 识别列、约束、选项的变更

4. **SQL生成**：
   - 生成对应的ALTER TABLE语句
   - 处理缺失表（DROP）和新增表（CREATE）

## 限制说明

- 目前仅支持InnoDB引擎
- 对于复杂的列类型定义，解析可能需要优化
- 暂不支持视图、存储过程等高级对象的对比

## 故障排除

### 连接失败

检查数据库连接参数是否正确：

```
user:password@host:port~database
```

### 解析错误

如遇到SQL解析问题，请检查：

- SQL文件编码是否为UTF-8
- CREATE TABLE语句是否规范

## 开发

### 运行测试

```bash
cargo test
```

### 构建调试版本

```bash
cargo build
```

## License

MIT

## 贡献

欢迎提交Issue和Pull Request！

## 更新日志

### v1.1.1

- 修复了表定义解析的正则表达式
- 改进了约束对比逻辑
- 优化了SQL生成格式

## 联系方式

如有问题或建议，请提交Issue。
