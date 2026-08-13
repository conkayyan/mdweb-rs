fn main() {
    let src = "graph TB\n\
               \x20 A((开始)) --> B\n\
               \x20 A --> C\n\
               \x20 A --> D\n\
               \x20 A --> E\n\
               \x20 A --> F\n\
               \x20 A --> G\n\
               \x20 A --> H\n\
               \x20 B --> I[结果]\n\
               \x20 C --> I\n\
               \x20 D --> I\n\
               \x20 E --> I\n\
               \x20 F --> I\n\
               \x20 G --> I\n\
               \x20 H --> I\n";
    let svg = mdweb::diagram::flowchart::render(src).unwrap();
    std::fs::write("/tmp/fan.svg", &svg).unwrap();
}
