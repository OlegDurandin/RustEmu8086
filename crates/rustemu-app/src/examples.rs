//! Example programs bundled with the application.

pub const NEW_TEMPLATE: &str = "\
; COM program template
#make_COM#
org 100h

        ; add your code here

        ret
";

macro_rules! example {
    ($name:literal) => {
        ($name, include_str!(concat!("../../../examples/", $name)))
    };
}

pub const EXAMPLES: &[(&str, &str)] = &[
    example!("hello.asm"),
    example!("hello_exe.asm"),
    example!("echo.asm"),
    example!("calc.asm"),
    example!("factorial.asm"),
    example!("bubble_sort.asm"),
    example!("keyboard.asm"),
    example!("colors.asm"),
    example!("graphics.asm"),
    example!("traffic_lights.asm"),
    example!("stepper_motor.asm"),
    example!("led_display.asm"),
    example!("thermometer.asm"),
    example!("printer.asm"),
    example!("robot.asm"),
];
