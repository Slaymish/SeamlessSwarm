#ifndef ATECC608_H
#define ATECC608_H

#include <stdint.h>
#include <stdbool.h>

#define ATECC608_I2C_ADDR 0x60
#define ATECC608_KEY_SLOT 0

typedef struct {
    uint8_t address;
    bool initialized;
} atecc608_device_t;

int atecc608_init(atecc608_device_t *dev, uint8_t i2c_address);
int atecc608_read_serial(atecc608_device_t *dev, uint8_t *serial_out, size_t max_len);
int atecc608_sign_challenge(atecc608_device_t *dev, const uint8_t *challenge, size_t challenge_len, uint8_t *signature_out, size_t *sig_len_io);

#endif
