---
title: "安装 mdweb"
date: "2026-08-04"
tags: ["tutorial", "setup"]
---

mdweb 是单一静态二进制。从 GitHub 下载 release，或用
`cargo install mdweb` 从源码构建。加入 `$PATH` 后执行：

```bash
mdweb create my-blog
cd my-blog
mdweb run
```

打开 <http://127.0.0.1:8080> 即可看到演示站点。