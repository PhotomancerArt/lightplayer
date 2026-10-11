#IF riscv
.trap : ALIGN(4)
{
  _trap_section_origin = .;
  KEEP(*(.trap));
  *(.trap.*);
} > RWTEXT
#ENDIF

.rwtext : ALIGN(4)
{
  . = ALIGN (4);
  *(.rwtext.literal .rwtext .rwtext.literal.* .rwtext.*)
  /* unconditionally add patched SPI-flash ROM functions (from esp-rom-sys) - the linker is still happy if there are none */
  *:esp_rom_spiflash.*(.literal .literal.* .text .text.*)

  #IF ESP_HAL_CONFIG_USE_RWTEXT_LD_HOOK
    INCLUDE "rwtext_hook.x"
  #ENDIF

  . = ALIGN(4);
} > RWTEXT

.rwtext.wifi :
{
  . = ALIGN(4);
  /* LP fork (fifth diff, README-LP.md): each optional class below can be
     sent to flash `.text` (text.x) instead; the default is RAM, as upstream. */
  #IF ESP_HAL_CONFIG_PLACE_WIFI_IRAM_IN_FLASH
  #ELSE
  *( .wifi0iram  .wifi0iram.*)
  #ENDIF
  #IF ESP_HAL_CONFIG_PLACE_WIFI_RX_SLP_IRAM_IN_FLASH
  #ELSE
  *( .wifirxiram  .wifirxiram.*)
  *( .wifislprxiram  .wifislprxiram.*)
  *( .wifislpiram  .wifislpiram.*)
  #ENDIF
  *( .phyiram  .phyiram.*)
  #IF ESP_HAL_CONFIG_PLACE_BLE_CONTROLLER_IRAM_IN_FLASH
  /* EXCLUDE_FILE inside the list, once per pattern: lld reads a leading
     `EXCLUDE_FILE(...) *(...)` as a file pattern named EXCLUDE_FILE. */
  *( EXCLUDE_FILE(*libble_app.a:*) .iram1  EXCLUDE_FILE(*libble_app.a:*) .iram1.*)
  #ELSE
  *( .iram1  .iram1.*)
  #ENDIF
  #IF ESP_HAL_CONFIG_PLACE_WIFI_IRAM_IN_FLASH
  #ELSE
  *( .wifiextrairam.* )
  *( .coexiram.* )
  #ENDIF
  *( .high_perf_code_iram* )
  *( .coexsleepiram* )
  *( .wifiorslpiram* )
  *( .isr_iram* )
  *( .conn_iram* )
  *( .sleep_iram* )
  . = ALIGN(4);

  _rwtext_len = . - ORIGIN(RWTEXT);
} > RWTEXT
