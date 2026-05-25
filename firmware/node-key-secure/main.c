#include <stdio.h>
#include <string.h>
#include "atecc608.h"

int main(void) {
    atecc608_device_t dev;
    uint8_t serial[9];
    uint8_t challenge[32] = {0x01, 0x02, 0x03, 0x04};
    uint8_t signature[64];
    size_t sig_len = sizeof(signature);

    if (atecc608_init(&dev, ATECC608_I2C_ADDR) != 0) {
        printf("Init failed\n");
        return 1;
    }

    if (atecc608_read_serial(&dev, serial, sizeof(serial)) != 0) {
        printf("Read serial failed\n");
        return 1;
    }

    printf("Serial: ");
    for (int i = 0; i < 9; i++) {
        printf("%02X", serial[i]);
    }
    printf("\n");

    if (atecc608_sign_challenge(&dev, challenge, sizeof(challenge), signature, &sig_len) != 0) {
        printf("Signing failed\n");
        return 1;
    }

    printf("Signature calculated successfully. Length: %zu\n", sig_len);
    return 0;
}
