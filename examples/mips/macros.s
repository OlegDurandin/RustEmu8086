# Macros: printing macros come from another file (.include), and a macro of this
# program runs a loop with another macro as its body.
        .include "print_macros.s"

# for (%reg, %from, %to, %body): runs macro %body for %reg = %from..%to.
.macro for (%reg, %from, %to, %body)
        add  %reg, $zero, %from
loop:
        %body ()
        add  %reg, %reg, 1
        ble  %reg, %to, loop    # loop is loop_M<n> in each expansion
.end_macro

.macro square ()
        mul  $t1, $t0, $t0
        print_int ($t1)
        print_str (" ")
.end_macro

        .text
main:   print_str ("Squares: ")
        for ($t0, 1, 5, square)
        print_str ("\n")
        done
