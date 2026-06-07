<template>
  <div class="empty-rooms">
    <i class="bi bi-door-open"></i>
    <p>暂无房间</p>
    <span>创建一个房间或加入已有房间</span>
  </div>
</template>

<style scoped>
.empty-rooms {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--text-muted);
}

.empty-rooms i {
  font-size: 48px;
  margin-bottom: 8px;
  opacity: 0.4;
}

.empty-rooms p {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-secondary);
  margin: 0;
}

.empty-rooms span {
  font-size: 13px;
}
</style>
