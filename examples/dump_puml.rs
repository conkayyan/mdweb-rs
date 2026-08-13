//! Temporary: dump the vehicle PlantUML SVG.

use mdweb::diagram::plantuml::render;

const SRC: &str = r#"@startuml
interface Vehicle {
  + start()
  + stop()
}

abstract class AbstractCar {
  - model: String
  # speed: int
  + AbstractCar(model: String)
  + accelerate()
  + getModel(): String
}

class Sedan {
  + Sedan(model: String)
  + openTrunk()
}

class SUV {
  + SUV(model: String)
  + enable4WD()
}

class Engine {
  - type: String
  - horsePower: int
  + Engine(type: String, hp: int)
  + start()
  + stop()
}

class Wheel {
  - size: int
  - brand: String
  + Wheel(size: int, brand: String)
}

class Tire {
  - type: String
  + Tire(type: String)
}

class Driver {
  - name: String
  + Driver(name: String)
  + drive(car: AbstractCar)
}

class Manufacturer {
  + buildCar(): AbstractCar
}

' Relations
Vehicle <|.. AbstractCar   ' 实现
AbstractCar <|-- Sedan     ' 继承
AbstractCar <|-- SUV       ' 继承
AbstractCar *-- Engine     ' 组合
AbstractCar o-- Wheel      ' 聚合
Wheel *-- Tire             ' 组合
AbstractCar <--> Driver    ' 双向关联
AbstractCar ..> Manufacturer : depends on
note right of Manufacturer : 制造工厂
note left of Sedan : 轿车
note right of SUV : 越野车
@enduml"#;

fn main() {
    match render(SRC) {
        Some(svg) => println!("{svg}"),
        None => eprintln!("(render returned None)"),
    }
}
