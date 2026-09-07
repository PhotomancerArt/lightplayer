.text : {
  *(.literal.*fw_esp32v3* .text.*fw_esp32v3*)
  *(.literal.*esp_hal* .text.*esp_hal*)
  *(.literal.*esp_rtos* .text.*esp_rtos*)
  *(.text.pad)
}
