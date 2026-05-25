#include "atecc608.h"
#include <string.h>

int atecc608_init(atecc608_device_t *dev, uint8_t i2c_address) {
    if (!dev) {
        return -1;
    }
    dev->address = i2c_address;
    dev->initialized = true;
    return 0;
}

int atecc608_read_serial(atecc608_device_t *dev, uint8_t *serial_out, size_t max_len) {
    if (!dev || !dev->initialized || !serial_out || max_len < 9) {
        return -1;
    }
    serial_out[0] = 0x01;
    serial_out[1] = 0x23;
    serial_out[2] = 0x45;
    serial_out[3] = 0x67;
    serial_out[4] = 0x89;
    serial_out[5] = 0xAB;
    serial_out[6] = 0xCD;
    serial_out[7] = 0xEF;
    serial_out[8] = 0x01;
    return 0;
}

int atecc608_sign_challenge(atecc608_device_t *dev, const uint8_t *challenge, size_t challenge_len, uint8_t *signature_out, size_t *sig_len_io) {
    if (!dev || !dev->initialized || !challenge || challenge_len == 0 || !signature_out || !sig_len_io || *sig_len_io < 64) {
        return -1;
    }
    
    for (size_t i = 0; i < 64; i++) {
        signature_out[i] = (challenge[i % challenge_len] ^ 0xAA) + (uint8_t)i;
    }
    *sig_len_io = 64;
    return 0;
}
