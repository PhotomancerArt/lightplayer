

SECTIONS {

  .text : ALIGN(4)
  {
    #IF riscv
    KEEP(*(.init));
    KEEP(*(.init.rust));
    KEEP(*(.text.abort));
    #ENDIF
    *(.literal .text .literal.* .text.*)
    /* LP fork (fifth diff, README-LP.md): the optional radio IRAM classes
       that rwtext.x leaves out when their switch is on. */
    #IF ESP_HAL_CONFIG_PLACE_WIFI_IRAM_IN_FLASH
    *(.wifi0iram .wifi0iram.* .wifiextrairam.* .coexiram.*)
    #ENDIF
    #IF ESP_HAL_CONFIG_PLACE_WIFI_RX_SLP_IRAM_IN_FLASH
    *(.wifirxiram .wifirxiram.* .wifislprxiram .wifislprxiram.* .wifislpiram .wifislpiram.*)
    #ENDIF
    #IF ESP_HAL_CONFIG_PLACE_BLE_CONTROLLER_IRAM_IN_FLASH
    *libble_app.a:*(.iram1 .iram1.*)
    #ENDIF
  } > ROTEXT

}