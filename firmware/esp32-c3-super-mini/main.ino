/*
 * SDCARD BRIDGE
 * Device           : ESP32-C3 Super Mini
 * Tools            : Arduino IDE 2.3.6
 * Environtment     : v3.3.11
 * Schema Partition : Minimal SPIFFS (1.9MB APP with OTA/128KB SPIFFS)
 * Author           : Yahya Zulfikri
 * Created          : Agustus 2026
 * Updated          : Agustus 2026
 * Version          : 1.0.0
 */

#include <SPI.h>
#include <SdFat.h>

// ---------------- Pin config ----------------
#define PIN_SPI_SCK   4
#define PIN_SPI_MOSI  6
#define PIN_SPI_MISO  5
#define PIN_SD_CS     1

#define PIN_LED       8       // built-in LED, active LOW on most Super Mini boards
#define LED_ON        LOW
#define LED_OFF       HIGH

#define SPI_SPEED     SD_SCK_MHZ(25)

SdFat sd;
SdFile file;

// ---------------- Protocol constants ----------------
enum Cmd : uint8_t {
  CMD_PING            = 0x01,
  CMD_SD_INFO         = 0x02,
  CMD_LIST_DIR        = 0x03,
  CMD_READ_FILE_INIT  = 0x04,
  CMD_READ_CHUNK      = 0x05,
  CMD_WRITE_FILE_INIT = 0x06,
  CMD_WRITE_CHUNK     = 0x07,
  CMD_RAW_BLOCK_WRITE = 0x08,
  CMD_RAW_BLOCK_READ  = 0x09,
  CMD_FLASH_FINALIZE  = 0x0A,
};

enum Status : uint8_t {
  STATUS_OK    = 0x00,
  STATUS_ERROR = 0x01,
};

#define CHUNK_SIZE   512
#define SERIAL_BAUD  921600
#define HEADER_LEN   5
#define MAX_PAYLOAD  4096   // C3 has less RAM than classic ESP32, keep buffer modest

// ---------------- CRC32 ----------------
uint32_t crc32_table[256];
bool crc32_table_ready = false;

void crc32_init() {
  for (uint32_t i = 0; i < 256; i++) {
    uint32_t c = i;
    for (int k = 0; k < 8; k++)
      c = (c & 1) ? (0xEDB88320 ^ (c >> 1)) : (c >> 1);
    crc32_table[i] = c;
  }
  crc32_table_ready = true;
}

uint32_t crc32_update(const uint8_t *buf, size_t len, uint32_t crc = 0xFFFFFFFF) {
  if (!crc32_table_ready) crc32_init();
  for (size_t i = 0; i < len; i++)
    crc = crc32_table[(crc ^ buf[i]) & 0xFF] ^ (crc >> 8);
  return crc;
}

uint32_t crc32_final(uint32_t crc) {
  return crc ^ 0xFFFFFFFF;
}

// ---------------- Low-level serial helpers ----------------
bool readExact(uint8_t *buf, size_t len, uint32_t timeout_ms = 5000) {
  size_t got = 0;
  uint32_t start = millis();
  while (got < len) {
    if (Serial.available()) {
      buf[got++] = Serial.read();
    } else if (millis() - start > timeout_ms) {
      return false;
    }
  }
  return true;
}

void sendFrame(uint8_t cmd, const uint8_t *payload, uint32_t len) {
  uint8_t header[HEADER_LEN];
  header[0] = cmd;
  header[1] = (len >> 24) & 0xFF;
  header[2] = (len >> 16) & 0xFF;
  header[3] = (len >> 8) & 0xFF;
  header[4] = len & 0xFF;

  uint32_t crc = crc32_update(header, HEADER_LEN);
  if (len > 0) crc = crc32_update(payload, len, crc);
  crc = crc32_final(crc);

  Serial.write(header, HEADER_LEN);
  if (len > 0) Serial.write(payload, len);

  uint8_t crcBytes[4] = {
    (uint8_t)(crc >> 24), (uint8_t)(crc >> 16),
    (uint8_t)(crc >> 8),  (uint8_t)(crc)
  };
  Serial.write(crcBytes, 4);
}

void sendStatus(uint8_t status, const char *msg = nullptr) {
  uint8_t payload[128];
  payload[0] = status;
  size_t msgLen = 0;
  if (msg) {
    msgLen = strlen(msg);
    if (msgLen > 126) msgLen = 126;
    memcpy(payload + 1, msg, msgLen);
  }
  sendFrame(0xFF, payload, 1 + msgLen);
}

// ---------------- Command handlers ----------------
void handlePing() {
  sendFrame(CMD_PING, (const uint8_t*)"PONG", 4);
}

void handleSdInfo() {
  uint32_t sizeMB = sd.card()->sectorCount() / 2048;
  uint8_t payload[16];
  payload[0] = sd.card()->type();
  memcpy(payload + 1, &sizeMB, 4);
  uint32_t sectorSize = 512;
  memcpy(payload + 5, &sectorSize, 4);
  sendFrame(CMD_SD_INFO, payload, 9);
}

void handleListDir(const uint8_t *payload, uint32_t len) {
  char path[128] = {0};
  memcpy(path, payload, min(len, (uint32_t)127));

  SdFile dir;
  if (!dir.open(path[0] ? path : "/", O_RDONLY)) {
    sendStatus(STATUS_ERROR, "open dir failed");
    return;
  }

  String listing = "";
  SdFile entry;
  char name[64];
  while (entry.openNext(&dir, O_RDONLY)) {
    entry.getName(name, sizeof(name));
    listing += String(name);
    listing += entry.isDir() ? "/\n" : "\n";
    entry.close();
  }
  dir.close();
  sendFrame(CMD_LIST_DIR, (const uint8_t*)listing.c_str(), listing.length());
}

uint32_t currentFileSize = 0;

void handleReadFileInit(const uint8_t *payload, uint32_t len) {
  char path[128] = {0};
  memcpy(path, payload, min(len, (uint32_t)127));

  if (file.isOpen()) file.close();
  if (!file.open(path, O_RDONLY)) {
    sendStatus(STATUS_ERROR, "file not found");
    return;
  }
  currentFileSize = file.fileSize();
  uint32_t chunkCount = (currentFileSize + CHUNK_SIZE - 1) / CHUNK_SIZE;

  uint8_t out[8];
  memcpy(out, &currentFileSize, 4);
  memcpy(out + 4, &chunkCount, 4);
  sendFrame(CMD_READ_FILE_INIT, out, 8);
}

void handleReadChunk(const uint8_t *payload, uint32_t len) {
  uint32_t index;
  memcpy(&index, payload, 4);

  uint8_t buf[CHUNK_SIZE];
  file.seekSet((uint64_t)index * CHUNK_SIZE);
  int n = file.read(buf, CHUNK_SIZE);
  if (n < 0) {
    sendStatus(STATUS_ERROR, "read failed");
    return;
  }
  sendFrame(CMD_READ_CHUNK, buf, n);
}

void handleWriteFileInit(const uint8_t *payload, uint32_t len) {
  uint32_t pathLen = len - 4;
  uint32_t fileSize;
  memcpy(&fileSize, payload, 4);
  char path[128] = {0};
  memcpy(path, payload + 4, min(pathLen, (uint32_t)127));

  if (file.isOpen()) file.close();
  if (!file.open(path, O_WRITE | O_CREAT | O_TRUNC)) {
    sendStatus(STATUS_ERROR, "open for write failed");
    return;
  }
  currentFileSize = fileSize;
  sendStatus(STATUS_OK);
}

void handleWriteChunk(const uint8_t *payload, uint32_t len) {
  uint32_t index;
  memcpy(&index, payload, 4);
  const uint8_t *data = payload + 4;
  uint32_t dataLen = len - 4;

  file.seekSet((uint64_t)index * CHUNK_SIZE);
  size_t written = file.write(data, dataLen);
  if (written != dataLen) {
    sendStatus(STATUS_ERROR, "write mismatch");
    return;
  }
  file.sync();
  sendStatus(STATUS_OK);
}

void handleRawBlockWrite(const uint8_t *payload, uint32_t len) {
  uint32_t lba;
  memcpy(&lba, payload, 4);
  const uint8_t *data = payload + 4;
  uint32_t dataLen = len - 4;

  if (dataLen != 512) {
    sendStatus(STATUS_ERROR, "block must be 512B");
    return;
  }
  if (!sd.card()->writeSector(lba, data)) {
    sendStatus(STATUS_ERROR, "writeSector failed");
    return;
  }
  sendStatus(STATUS_OK);
}

void handleRawBlockRead(const uint8_t *payload, uint32_t len) {
  uint32_t lba;
  memcpy(&lba, payload, 4);

  uint8_t buf[512];
  if (!sd.card()->readSector(lba, buf)) {
    sendStatus(STATUS_ERROR, "readSector failed");
    return;
  }
  sendFrame(CMD_RAW_BLOCK_READ, buf, 512);
}

void handleFlashFinalize() {
  sd.card()->syncDevice();
  sendStatus(STATUS_OK);
}

// ---------------- Main frame dispatcher ----------------
void processFrame() {
  uint8_t header[HEADER_LEN];
  if (!readExact(header, HEADER_LEN)) return;

  uint8_t cmd = header[0];
  uint32_t len = ((uint32_t)header[1] << 24) | ((uint32_t)header[2] << 16) |
                 ((uint32_t)header[3] << 8)  | header[4];

  if (len > MAX_PAYLOAD) return;

  static uint8_t payload[MAX_PAYLOAD];
  if (len > 0 && !readExact(payload, len)) return;

  uint8_t crcBytes[4];
  if (!readExact(crcBytes, 4)) return;
  uint32_t receivedCrc = ((uint32_t)crcBytes[0] << 24) | ((uint32_t)crcBytes[1] << 16) |
                         ((uint32_t)crcBytes[2] << 8) | crcBytes[3];

  uint32_t computedCrc = crc32_update(header, HEADER_LEN);
  if (len > 0) computedCrc = crc32_update(payload, len, computedCrc);
  computedCrc = crc32_final(computedCrc);

  if (computedCrc != receivedCrc) {
    sendStatus(STATUS_ERROR, "crc mismatch");
    return;
  }

  digitalWrite(PIN_LED, LED_ON);

  switch (cmd) {
    case CMD_PING:            handlePing(); break;
    case CMD_SD_INFO:         handleSdInfo(); break;
    case CMD_LIST_DIR:        handleListDir(payload, len); break;
    case CMD_READ_FILE_INIT:  handleReadFileInit(payload, len); break;
    case CMD_READ_CHUNK:      handleReadChunk(payload, len); break;
    case CMD_WRITE_FILE_INIT: handleWriteFileInit(payload, len); break;
    case CMD_WRITE_CHUNK:     handleWriteChunk(payload, len); break;
    case CMD_RAW_BLOCK_WRITE: handleRawBlockWrite(payload, len); break;
    case CMD_RAW_BLOCK_READ:  handleRawBlockRead(payload, len); break;
    case CMD_FLASH_FINALIZE:  handleFlashFinalize(); break;
    default:
      sendStatus(STATUS_ERROR, "unknown cmd");
  }

  digitalWrite(PIN_LED, LED_OFF);
}

// ---------------- Setup / loop ----------------
void setup() {
  pinMode(PIN_LED, OUTPUT);
  digitalWrite(PIN_LED, LED_OFF);

  Serial.begin(SERIAL_BAUD);
  while (!Serial) { delay(10); }

  crc32_init();

  SPI.begin(PIN_SPI_SCK, PIN_SPI_MISO, PIN_SPI_MOSI, PIN_SD_CS);

  if (!sd.begin(PIN_SD_CS, SPI_SPEED)) {
    while (true) {
      digitalWrite(PIN_LED, LED_ON);  delay(200);
      digitalWrite(PIN_LED, LED_OFF); delay(200);
    }
  }
}

void loop() {
  if (Serial.available() >= HEADER_LEN) {
    processFrame();
  }
}
